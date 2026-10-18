use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::Url;

use crate::{
    error::AppError, model::ConverterOptions, network::PublicNetwork,
    upstream::read_limited_response,
};

pub struct ConversionRequest {
    pub converter_url: Url,
    pub options: ConverterOptions,
    pub subscription_url: Url,
    pub rules_url: Url,
    pub rules_marker: String,
    pub filename: String,
}

#[derive(Clone)]
pub struct Converter {
    network: PublicNetwork,
    max_bytes: usize,
}

impl Converter {
    pub fn new(network: PublicNetwork, max_bytes: usize) -> Self {
        Self { network, max_bytes }
    }

    fn request_url(
        &self,
        url: &Url,
        subscription_url: &Url,
        rules_url: &Url,
        options: &ConverterOptions,
        filename: &str,
    ) -> Url {
        let mut url = url.clone();
        // 转换器按 URL 缓存规则文件，每次生成使用不同地址以读取最新脚本与策略。
        static GENERATIONS: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let generation = format!(
            "{timestamp}-{}",
            GENERATIONS.fetch_add(1, Ordering::Relaxed)
        );
        let mut rules_url = rules_url.clone();
        rules_url
            .query_pairs_mut()
            .append_pair("generation", &generation);
        {
            // 保留外部转换器自带的认证查询参数，转换参数由服务统一设置。
            let extra: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| {
                    ![
                        "target", "url", "config", "filename", "list", "insert", "emoji", "tfo",
                        "scv", "fdn", "expand", "sort", "new_name", "interval",
                    ]
                    .contains(&key.as_ref())
                })
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
            url.set_query(None);
            let mut pairs = url.query_pairs_mut();
            pairs.extend_pairs(extra);
            pairs.append_pair("target", "clash");
            pairs.append_pair("url", subscription_url.as_str());
            pairs.append_pair("config", rules_url.as_str());
            pairs.append_pair("filename", filename);
            pairs.append_pair("list", "false");
            for (key, value) in [
                ("insert", false),
                ("emoji", options.emoji),
                ("tfo", options.tfo),
                ("scv", options.scv),
                ("fdn", options.fdn),
                ("expand", options.expand),
                ("sort", options.sort),
                ("new_name", options.new_name),
            ] {
                pairs.append_pair(key, &value.to_string());
            }
            pairs.append_pair("interval", &options.interval_secs.to_string());
        }
        url
    }

    pub async fn generate(&self, request: &ConversionRequest) -> Result<Vec<u8>, AppError> {
        let url = self.request_url(
            &request.converter_url,
            &request.subscription_url,
            &request.rules_url,
            &request.options,
            &request.filename,
        );
        let response = self.network.get(url).await?;
        read_limited_response(response, self.max_bytes).await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use super::*;

    #[test]
    fn custom_filename_replaces_converter_default() {
        let converter = Converter::new(PublicNetwork::new(Duration::from_secs(1)), 1024);
        let url = Url::parse("https://converter.example.com/sub?token=abc&filename=old").unwrap();
        let subscription = Url::parse("https://sub.example.com/api/v1/subscription/abc").unwrap();
        let rules = Url::parse("https://sub.example.com/api/v1/rules/def").unwrap();
        let output = converter.request_url(
            &url,
            &subscription,
            &rules,
            &ConverterOptions::default(),
            "我的 订阅",
        );
        let pairs: HashMap<_, _> = output.query_pairs().into_owned().collect();
        assert_eq!(pairs.get("filename").map(String::as_str), Some("我的 订阅"));
        assert_eq!(pairs.get("token").map(String::as_str), Some("abc"));
    }
}
