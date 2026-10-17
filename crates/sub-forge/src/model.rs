use serde::{Deserialize, Serialize};

use crate::{error::AppError, network::PublicNetwork};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub rules: RuleSettings,
    #[serde(default)]
    pub converter: ConverterOptions,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSettings {
    pub source_url: String,
}

impl Policy {
    fn normalize(&mut self) -> Result<(), AppError> {
        self.rules.source_url = PublicNetwork::parse(&self.rules.source_url)?.to_string();
        if !(60..=604800).contains(&self.converter.interval_secs) {
            return Err(AppError::BadRequest(
                "刷新间隔须在 60 至 604800 秒之间".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConverterOptions {
    pub emoji: bool,
    pub tfo: bool,
    pub scv: bool,
    pub fdn: bool,
    pub expand: bool,
    pub sort: bool,
    pub new_name: bool,
    pub interval_secs: u64,
}

impl Default for ConverterOptions {
    fn default() -> Self {
        Self {
            emoji: true,
            tfo: false,
            scv: false,
            fdn: false,
            expand: true,
            sort: false,
            new_name: true,
            interval_secs: 86400,
        }
    }
}

pub const MAX_SCRIPTS_BYTES: usize = 64 * 1024;

// 用户代码与订阅参数一起保存，保留源码注释和排版。
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserScripts {
    #[serde(default)]
    pub subscription: String,
    #[serde(default)]
    pub preprocess_rules: String,
    #[serde(default)]
    pub postprocess_config: String,
}

impl UserScripts {
    pub fn validate(&self) -> Result<(), AppError> {
        let total =
            self.subscription.len() + self.preprocess_rules.len() + self.postprocess_config.len();
        if total > MAX_SCRIPTS_BYTES {
            return Err(AppError::BadRequest("脚本总长度不能超过 64 KiB".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionConfig {
    pub source_url: String,
    pub converter_url: String,
    pub policy: Policy,
    #[serde(default)]
    pub scripts: UserScripts,
}

impl SubscriptionConfig {
    pub fn normalize(&mut self) -> Result<(), AppError> {
        self.source_url = PublicNetwork::parse(&self.source_url)?.to_string();
        self.converter_url = PublicNetwork::parse(&self.converter_url)?.to_string();
        self.policy.normalize()?;
        self.scripts.validate()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub source_url: String,
    pub script: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "config", rename_all = "snake_case")]
pub enum StoredLink {
    Feed(SubscriptionConfig),
    Subscription(SourceConfig),
    Rules(SourceConfig),
    RuleResource { source_url: String },
}

#[derive(Clone, Copy)]
pub enum LinkKind {
    Feed,
    Subscription,
    Rules,
    RuleResource,
}

impl LinkKind {
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Feed => "clash",
            Self::Subscription => "subscription",
            Self::Rules => "rules",
            Self::RuleResource => "rule-resource",
        }
    }
}

impl StoredLink {
    pub fn kind(&self) -> LinkKind {
        match self {
            Self::Feed(_) => LinkKind::Feed,
            Self::Subscription(_) => LinkKind::Subscription,
            Self::Rules(_) => LinkKind::Rules,
            Self::RuleResource { .. } => LinkKind::RuleResource,
        }
    }
}
