use serde::{Deserialize, Serialize};

use super::{Scope, Ticket};
use crate::{
    error::AppError,
    policy::{ConverterOptions, Policy, RuleSettings},
    user_scripts::UserScripts,
};

// 链接格式与 frontend/generator-worker.js 一致；前端编码订阅，核心编码回调。
#[derive(Serialize, Deserialize)]
#[serde(tag = "k", deny_unknown_fields)]
pub(super) enum CompactTicket {
    #[serde(rename = "f")]
    Feed {
        #[serde(rename = "s")]
        source_url: String,
        #[serde(rename = "c")]
        converter_url: String,
        #[serde(rename = "r")]
        rules_url: String,
        #[serde(rename = "o", default, skip_serializing_if = "is_default")]
        options: CompactOptions,
        #[serde(rename = "j", default, skip_serializing_if = "is_default")]
        scripts: CompactScripts,
    },
    #[serde(rename = "s")]
    Subscription {
        #[serde(rename = "s")]
        source_url: String,
        #[serde(rename = "j", default, skip_serializing_if = "String::is_empty")]
        script: String,
    },
    #[serde(rename = "r")]
    Rules {
        #[serde(rename = "s")]
        source_url: String,
        #[serde(rename = "j", default, skip_serializing_if = "String::is_empty")]
        script: String,
    },
    #[serde(rename = "l")]
    RuleResource {
        #[serde(rename = "s")]
        source_url: String,
    },
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

fn enabled() -> bool {
    true
}

fn is_enabled(value: &bool) -> bool {
    *value
}

fn interval() -> u64 {
    86400
}

fn is_default_interval(value: &u64) -> bool {
    *value == interval()
}

// v1 的缺省值属于编码协议，不跟随业务默认值变化。
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CompactOptions {
    #[serde(rename = "e", default = "enabled", skip_serializing_if = "is_enabled")]
    emoji: bool,
    #[serde(rename = "t", default, skip_serializing_if = "is_default")]
    tfo: bool,
    #[serde(rename = "s", default, skip_serializing_if = "is_default")]
    scv: bool,
    #[serde(rename = "f", default, skip_serializing_if = "is_default")]
    fdn: bool,
    #[serde(rename = "x", default = "enabled", skip_serializing_if = "is_enabled")]
    expand: bool,
    #[serde(rename = "o", default, skip_serializing_if = "is_default")]
    sort: bool,
    #[serde(rename = "n", default = "enabled", skip_serializing_if = "is_enabled")]
    new_name: bool,
    #[serde(
        rename = "i",
        default = "interval",
        skip_serializing_if = "is_default_interval"
    )]
    interval_secs: u64,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self {
            emoji: true,
            tfo: false,
            scv: false,
            fdn: false,
            expand: true,
            sort: false,
            new_name: true,
            interval_secs: interval(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CompactScripts {
    #[serde(rename = "s", default, skip_serializing_if = "String::is_empty")]
    subscription: String,
    #[serde(rename = "r", default, skip_serializing_if = "String::is_empty")]
    preprocess_rules: String,
    #[serde(rename = "c", default, skip_serializing_if = "String::is_empty")]
    postprocess_config: String,
}

impl TryFrom<&Ticket> for CompactTicket {
    type Error = AppError;

    fn try_from(ticket: &Ticket) -> Result<Self, Self::Error> {
        let source_url = ticket.source_url.clone().ok_or(AppError::Internal)?;
        Ok(match ticket.scope {
            Scope::Feed => {
                let policy = ticket.policy.as_ref().ok_or(AppError::Internal)?;
                let scripts = ticket.scripts.as_ref().ok_or(AppError::Internal)?;
                let options = &policy.converter;
                Self::Feed {
                    source_url,
                    converter_url: ticket.converter_url.clone().ok_or(AppError::Internal)?,
                    rules_url: policy.rules.source_url.clone(),
                    options: CompactOptions {
                        emoji: options.emoji,
                        tfo: options.tfo,
                        scv: options.scv,
                        fdn: options.fdn,
                        expand: options.expand,
                        sort: options.sort,
                        new_name: options.new_name,
                        interval_secs: options.interval_secs,
                    },
                    scripts: CompactScripts {
                        subscription: scripts.subscription.clone(),
                        preprocess_rules: scripts.preprocess_rules.clone(),
                        postprocess_config: scripts.postprocess_config.clone(),
                    },
                }
            }
            Scope::Subscription => Self::Subscription {
                source_url,
                script: ticket.script.clone().ok_or(AppError::Internal)?,
            },
            Scope::Rules => Self::Rules {
                source_url,
                script: ticket.script.clone().ok_or(AppError::Internal)?,
            },
            Scope::RuleResource => Self::RuleResource { source_url },
        })
    }
}

impl From<CompactTicket> for Ticket {
    fn from(compact: CompactTicket) -> Self {
        let (scope, source_url, script) = match compact {
            CompactTicket::Feed {
                source_url,
                converter_url,
                rules_url,
                options,
                scripts,
            } => {
                return Self {
                    scope: Scope::Feed,
                    source_url: Some(source_url),
                    converter_url: Some(converter_url),
                    policy: Some(Policy {
                        rules: RuleSettings {
                            source_url: rules_url,
                        },
                        converter: ConverterOptions {
                            emoji: options.emoji,
                            tfo: options.tfo,
                            scv: options.scv,
                            fdn: options.fdn,
                            expand: options.expand,
                            sort: options.sort,
                            new_name: options.new_name,
                            interval_secs: options.interval_secs,
                        },
                    }),
                    scripts: Some(UserScripts {
                        subscription: scripts.subscription,
                        preprocess_rules: scripts.preprocess_rules,
                        postprocess_config: scripts.postprocess_config,
                    }),
                    script: None,
                };
            }
            CompactTicket::Subscription { source_url, script } => {
                (Scope::Subscription, source_url, Some(script))
            }
            CompactTicket::Rules { source_url, script } => (Scope::Rules, source_url, Some(script)),
            CompactTicket::RuleResource { source_url } => (Scope::RuleResource, source_url, None),
        };
        Self {
            scope,
            source_url: Some(source_url),
            converter_url: None,
            scripts: None,
            policy: None,
            script,
        }
    }
}
