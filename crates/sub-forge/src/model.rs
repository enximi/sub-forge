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
    pub provider: bool,
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
            provider: false,
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

pub const DEFAULT_FILENAME: &str = "sub-forge";
const MAX_FILENAME_CHARS: usize = 64;

fn default_filename() -> String {
    DEFAULT_FILENAME.to_owned()
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
    #[serde(default = "default_filename")]
    pub filename: String,
    pub policy: Policy,
    #[serde(default)]
    pub scripts: UserScripts,
}

impl SubscriptionConfig {
    pub fn normalize(&mut self) -> Result<(), AppError> {
        self.source_url = PublicNetwork::parse(&self.source_url)?.to_string();
        self.converter_url = PublicNetwork::parse(&self.converter_url)?.to_string();
        self.filename = normalize_filename(&self.filename)?;
        self.policy.normalize()?;
        self.scripts.validate()
    }
}

/// 文件名会进入响应头和转换器参数，只允许安全的单段名称，不包含扩展名。
fn normalize_filename(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.is_empty() {
        return Ok(default_filename());
    }
    if name.chars().count() > MAX_FILENAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "文件名不能超过 {MAX_FILENAME_CHARS} 个字符"
        )));
    }
    if name == "." || name == ".." {
        return Err(AppError::BadRequest("文件名不能是 . 或 ..".into()));
    }
    if let Some(character) = name.chars().find(|character| {
        character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
            || matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
    }) {
        return Err(AppError::BadRequest(format!(
            "文件名不能包含 {character:?}、路径分隔符或控制字符"
        )));
    }
    let lowercase = name.to_lowercase();
    if [".yaml", ".yml", ".txt", ".conf", ".ini"]
        .iter()
        .any(|extension| lowercase.ends_with(extension))
    {
        return Err(AppError::BadRequest(
            "文件名请填写不含扩展名的名称，扩展名由服务自动添加".into(),
        ));
    }
    Ok(name.to_owned())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub source_url: String,
    pub script: String,
    // 规则回调不需要文件名，省略后保持与旧记录相同的去重键。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filename: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_defaults_when_missing_or_blank() {
        assert_eq!(normalize_filename("").unwrap(), DEFAULT_FILENAME);
        assert_eq!(normalize_filename("   ").unwrap(), DEFAULT_FILENAME);
        let config: SubscriptionConfig = serde_json::from_value(serde_json::json!({
            "source_url": "https://example.com/sub",
            "converter_url": "https://example.com/converter",
            "policy": { "rules": { "source_url": "https://example.com/rules.ini" } }
        }))
        .unwrap();
        assert_eq!(config.filename, DEFAULT_FILENAME);
    }

    #[test]
    fn filename_trims_and_keeps_unicode() {
        assert_eq!(normalize_filename("  我的订阅  ").unwrap(), "我的订阅");
        assert_eq!(normalize_filename("my.config").unwrap(), "my.config");
    }

    #[test]
    fn filename_rejects_unsafe_names() {
        for name in [
            "a/b",
            "a\\b",
            "a:b",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "a\nb",
            ".",
            "..",
            "sub-forge.yaml",
            "SUB.TXT",
            "config.ini",
        ] {
            assert!(
                normalize_filename(name).is_err(),
                "{name:?} should be rejected"
            );
        }
        assert!(normalize_filename(&"长".repeat(MAX_FILENAME_CHARS + 1)).is_err());
    }

    #[test]
    fn old_source_configs_default_to_empty_filename() {
        let config: SourceConfig = serde_json::from_value(serde_json::json!({
            "source_url": "https://example.com/sub",
            "script": ""
        }))
        .unwrap();
        assert!(config.filename.is_empty());
    }
}
