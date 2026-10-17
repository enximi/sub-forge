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
    pub fn validate(&self) -> Result<(), AppError> {
        PublicNetwork::parse(&self.rules.source_url)?;
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
