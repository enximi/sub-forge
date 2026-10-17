use std::time::Duration;

use reqwest::header::HeaderValue;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    config::RuntimeConfig,
    converter::{ConversionRequest, Converter},
    error::AppError,
    links::{Links, SavedLink},
    model::{LinkKind, SourceConfig, StoredLink, SubscriptionConfig},
    network::PublicNetwork,
    pipeline::ConfigPipeline,
    rules::{RuleSource, template_marker},
    scripts::ScriptEngine,
    subscription::SubscriptionSource,
};

pub struct SubscriptionContent {
    pub body: Vec<u8>,
    pub userinfo: Option<HeaderValue>,
}

pub struct SubscriptionService {
    links: Links,
    scripts: ScriptEngine,
    subscriptions: SubscriptionSource,
    rules: RuleSource,
    pipeline: ConfigPipeline,
    conversions: Semaphore,
    fetches: Semaphore,
    timeout: Duration,
    max_subscription_bytes: usize,
}

impl SubscriptionService {
    pub fn new(config: &RuntimeConfig, links: Links) -> Self {
        let network = PublicNetwork::new(config.request_timeout);
        Self {
            rules: RuleSource::new(
                PublicNetwork::new(config.rules_timeout),
                config.max_rules_bytes,
                links.clone(),
            ),
            subscriptions: SubscriptionSource::new(network.clone(), config.max_subscription_bytes),
            pipeline: ConfigPipeline::new(
                Converter::new(network, config.max_config_bytes),
                config.max_config_bytes,
            ),
            scripts: ScriptEngine::new(config),
            links,
            conversions: Semaphore::new(config.conversion_concurrency),
            fetches: Semaphore::new(config.fetch_concurrency),
            timeout: config.request_timeout,
            max_subscription_bytes: config.max_subscription_bytes,
        }
    }

    pub async fn create(&self, mut config: SubscriptionConfig) -> Result<SavedLink, AppError> {
        let _permit = self.fetches.try_acquire().map_err(|_| AppError::Busy)?;
        config.normalize()?;
        self.scripts.validate_sources(&config.scripts).await?;
        self.links.save(StoredLink::Feed(config)).await
    }

    pub async fn generate(&self, id: &str) -> Result<Vec<u8>, AppError> {
        let _permit = self.conversions.try_acquire().map_err(|_| AppError::Busy)?;
        tokio::time::timeout(self.timeout, async {
            let config = self.links.feed(id).await?;
            let request = self.conversion_request(&config).await?;
            self.pipeline.generate(
                &config.scripts.postprocess_config,
                &self.scripts,
                script_context(json!({"subscription": request.subscription_url.as_str(), "rules": request.rules_url.as_str()})),
                &request,
            ).await
        }).await.map_err(|_| AppError::Busy)?
    }

    pub async fn subscription(&self, id: &str) -> Result<SubscriptionContent, AppError> {
        let _permit = self.fetches.try_acquire().map_err(|_| AppError::Busy)?;
        let config = self.links.source(id, LinkKind::Subscription).await?;
        let (mut body, headers) = self
            .subscriptions
            .fetch(PublicNetwork::parse(&config.source_url)?)
            .await?;
        if !config.script.trim().is_empty() {
            body = self.scripts.transform_text(
                &config.script, "subscription",
                String::from_utf8(body).map_err(|_| AppError::InvalidEncoding)?,
                script_context(json!({"subscription": self.links.url(LinkKind::Subscription, id).as_str()})),
                self.max_subscription_bytes,
            ).await?.into_bytes();
        }
        Ok(SubscriptionContent {
            body,
            userinfo: headers.get("subscription-userinfo").cloned(),
        })
    }

    pub async fn rules(&self, id: &str) -> Result<String, AppError> {
        let _permit = self.fetches.try_acquire().map_err(|_| AppError::Busy)?;
        let config = self.links.source(id, LinkKind::Rules).await?;
        let mut content = self
            .rules
            .config(
                &config,
                &self.scripts,
                script_context(json!({"rules": self.links.url(LinkKind::Rules, id).as_str()})),
            )
            .await?;
        // 转换器取模板失败时可能静默退回默认配置，标记用于确认本次模板被采用。
        content.push_str(&format!(
            "custom_proxy_group={}`select`[]DIRECT\n",
            template_marker(id)
        ));
        Ok(content)
    }

    pub async fn rule_resource(&self, id: &str) -> Result<Vec<u8>, AppError> {
        let _permit = self.fetches.try_acquire().map_err(|_| AppError::Busy)?;
        let source = self.links.resource(id).await?;
        self.rules.resource(PublicNetwork::parse(&source)?).await
    }

    async fn conversion_request(
        &self,
        config: &SubscriptionConfig,
    ) -> Result<ConversionRequest, AppError> {
        let subscription = self
            .links
            .save(StoredLink::Subscription(SourceConfig {
                source_url: config.source_url.clone(),
                script: config.scripts.subscription.clone(),
            }))
            .await?;
        let rules = self
            .links
            .save(StoredLink::Rules(SourceConfig {
                source_url: config.policy.rules.source_url.clone(),
                script: config.scripts.preprocess_rules.clone(),
            }))
            .await?;
        Ok(ConversionRequest {
            converter_url: PublicNetwork::parse(&config.converter_url)?,
            options: config.policy.converter.clone(),
            subscription_url: self.links.url(LinkKind::Subscription, &subscription.id),
            rules_url: self.links.url(LinkKind::Rules, &rules.id),
            rules_marker: template_marker(&rules.id),
        })
    }
}

fn script_context(urls: Value) -> Value {
    json!({"version": 1, "urls": urls, "secrets": {}})
}
