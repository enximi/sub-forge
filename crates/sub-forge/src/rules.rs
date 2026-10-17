use std::hash::{DefaultHasher, Hash, Hasher};

use reqwest::Url;
use serde_json::Value;

use crate::{
    error::AppError,
    links::Links,
    model::{SourceConfig, StoredLink},
    network::PublicNetwork,
    scripts::ScriptEngine,
    upstream::read_limited_response,
};

#[derive(Clone)]
pub struct RuleSource {
    network: PublicNetwork,
    max_bytes: usize,
    links: Links,
}

impl RuleSource {
    pub fn new(network: PublicNetwork, max_bytes: usize, links: Links) -> Self {
        Self {
            network,
            max_bytes,
            links,
        }
    }

    pub async fn resource(&self, url: Url) -> Result<Vec<u8>, AppError> {
        read_limited_response(self.network.get(url).await?, self.max_bytes).await
    }

    pub async fn config(
        &self,
        config: &SourceConfig,
        scripts: &ScriptEngine,
        context: Value,
    ) -> Result<String, AppError> {
        let source = self
            .resource(PublicNetwork::parse(&config.source_url)?)
            .await?;
        let source = String::from_utf8(source).map_err(|_| AppError::InvalidEncoding)?;
        let source = scripts
            .transform_text(
                &config.script,
                "preprocess_rules",
                source,
                context,
                self.max_bytes,
            )
            .await?;
        self.protect_template(&source).await
    }

    async fn protect_template(&self, source: &str) -> Result<String, AppError> {
        // 用户模板中的规则列表也走受限代理，转换器不能借模板访问私网或本地文件。
        let mut output = String::new();
        let mut found_custom = false;
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with([';', '#']) {
                continue;
            }
            if line.starts_with('[') {
                if line != "[custom]" || found_custom {
                    return Err(AppError::BadRequest(
                        "规则模板仅支持一个 [custom] 区段".into(),
                    ));
                }
                found_custom = true;
                output.push_str("[custom]\n");
                continue;
            }
            if !found_custom {
                return Err(AppError::BadRequest("规则模板缺少 [custom] 区段".into()));
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| AppError::BadRequest("规则模板包含无效指令".into()))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "ruleset" => {
                    let (group, resource) = value
                        .split_once(',')
                        .ok_or_else(|| AppError::BadRequest("ruleset 缺少策略名称".into()))?;
                    if resource.starts_with("[]") {
                        output.push_str(line);
                    } else {
                        let (prefix, resource) = [
                            "clash-domain:",
                            "clash-ipcidr:",
                            "clash-classic:",
                            "surge:",
                            "quanx:",
                        ]
                        .iter()
                        .find_map(|prefix| resource.strip_prefix(prefix).map(|url| (*prefix, url)))
                        .unwrap_or(("", resource));
                        let (resource, interval) =
                            resource.split_once(',').unwrap_or((resource, ""));
                        if !interval.is_empty() && interval.parse::<u64>().is_err() {
                            return Err(AppError::BadRequest("ruleset 刷新间隔无效".into()));
                        }
                        let resource = PublicNetwork::parse(resource)?;
                        self.network.validate(&resource).await?;
                        let link = self
                            .links
                            .save(StoredLink::RuleResource {
                                source_url: resource.to_string(),
                            })
                            .await?;
                        output.push_str(&format!("ruleset={group},{prefix}{}", link.url));
                        if !interval.is_empty() {
                            output.push(',');
                            output.push_str(interval);
                        }
                    }
                }
                "custom_proxy_group" => {
                    let kind = value.split('`').nth(1).unwrap_or("");
                    if !["select", "url-test", "fallback", "load-balance", "relay"].contains(&kind)
                        || value.to_ascii_lowercase().contains("script:")
                    {
                        return Err(AppError::BadRequest("规则模板的代理组类型不受支持".into()));
                    }
                    output.push_str(line);
                }
                "enable_rule_generator" | "overwrite_original_rules" => {
                    if !["true", "false"].contains(&value) {
                        return Err(AppError::BadRequest(
                            "规则生成开关须为 true 或 false".into(),
                        ));
                    }
                    output.push_str(line)
                }
                _ => {
                    return Err(AppError::BadRequest(
                        "公开服务的模板只支持 ruleset、custom_proxy_group 和规则生成开关".into(),
                    ));
                }
            }
            output.push('\n');
            if output.len() > self.max_bytes {
                return Err(AppError::ResponseTooLarge);
            }
        }
        if !found_custom {
            return Err(AppError::BadRequest("规则模板缺少 [custom] 区段".into()));
        }
        Ok(output)
    }
}

pub fn template_marker(id: &str) -> String {
    // 使用链接 ID 的摘要区分规则模板。
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    format!("sub-forge-check-{:016x}", hasher.finish())
}
