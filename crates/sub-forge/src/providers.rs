use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use crate::error::AppError;

fn invalid(message: &str) -> AppError {
    AppError::InvalidGeneratedConfig(message.into())
}

pub fn has_subscription_provider(document: &Value, source: &str) -> bool {
    document["proxy-providers"]
        .as_object()
        .is_some_and(|providers| {
            providers
                .values()
                .any(|provider| provider["url"].as_str() == Some(source))
        })
}

fn pattern(value: &Value, key: &str) -> Result<Option<Regex>, AppError> {
    value[key]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(|text| Regex::new(text).map_err(|_| invalid("invalid provider or group filter")))
        .transpose()
}

fn selected(name: &str, include: &Option<Regex>, exclude: &Option<Regex>) -> bool {
    include.as_ref().is_none_or(|regex| regex.is_match(name))
        && exclude.as_ref().is_none_or(|regex| !regex.is_match(name))
}

pub fn inline_subscription(
    document: &mut Value,
    content: &[u8],
    source: &str,
) -> Result<(), AppError> {
    let list: Value = serde_yaml_ng::from_slice(content)
        .map_err(|_| invalid("converter returned an invalid node list"))?;
    let nodes = list["proxies"]
        .as_array()
        .ok_or_else(|| invalid("converter did not return a proxies list"))?;
    if nodes.is_empty() {
        return Err(invalid("converter returned an empty node list"));
    }
    let providers: Vec<_> = document["proxy-providers"]
        .as_object()
        .ok_or_else(|| invalid("missing proxy-providers"))?
        .iter()
        .filter(|(_, provider)| provider["url"].as_str() == Some(source))
        .map(|(name, provider)| (name.clone(), provider.clone()))
        .collect();
    let has_other_providers =
        document["proxy-providers"].as_object().unwrap().len() > providers.len();
    let mut expanded = HashMap::new();
    let proxies = document
        .as_object_mut()
        .unwrap()
        .entry("proxies")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| invalid("proxies must be a list"))?;
    for (provider_name, provider) in &providers {
        let include = pattern(provider, "filter")?;
        let exclude = pattern(provider, "exclude-filter")?;
        let excluded_types = provider["exclude-type"].as_str().unwrap_or("");
        let mut names = Vec::new();
        for node in nodes {
            let mut node = node.clone();
            let original_name = node["name"]
                .as_str()
                .ok_or_else(|| invalid("proxy is missing its name"))?;
            if !selected(original_name, &include, &exclude)
                || excluded_types
                    .split('|')
                    .any(|kind| !kind.is_empty() && node["type"].as_str() == Some(kind))
            {
                continue;
            }
            let mut name = original_name.to_owned();
            if let Some(overrides) = provider["override"].as_object() {
                // 名称改写先于组筛选，普通覆盖项沿用客户端的字段名。
                if let Some(renames) = overrides.get("proxy-name").and_then(Value::as_array) {
                    for rename in renames {
                        let regex = Regex::new(
                            rename["pattern"]
                                .as_str()
                                .ok_or_else(|| invalid("invalid proxy rename"))?,
                        )
                        .map_err(|_| invalid("invalid proxy rename pattern"))?;
                        name = regex
                            .replace_all(
                                &name,
                                rename["target"]
                                    .as_str()
                                    .ok_or_else(|| invalid("invalid proxy rename target"))?,
                            )
                            .into_owned();
                    }
                }
                name = format!(
                    "{}{}{}",
                    overrides
                        .get("additional-prefix")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    name,
                    overrides
                        .get("additional-suffix")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                );
                for (key, value) in overrides {
                    if !["proxy-name", "additional-prefix", "additional-suffix"]
                        .contains(&key.as_str())
                    {
                        node[key] = value.clone();
                    }
                }
            }
            node["name"] = name.clone().into();
            if let Some(existing) = proxies
                .iter()
                .find(|proxy| proxy["name"].as_str() == Some(&name))
            {
                if existing != &node {
                    return Err(invalid("conflicting proxy names while expanding provider"));
                }
            } else {
                proxies.push(node);
            }
            names.push(name);
        }
        expanded.insert(provider_name.clone(), names);
    }
    if let Some(groups) = document["proxy-groups"].as_array_mut() {
        for group in groups {
            if !group.is_object() {
                return Err(invalid("proxy group must be a mapping"));
            }
            let include = pattern(group, "filter")?;
            let exclude = pattern(group, "exclude-filter")?;
            let mut names = Vec::new();
            let all_providers = group["include-all-providers"].as_bool() == Some(true);
            let all = group["include-all"].as_bool() == Some(true);
            if let Some(used) = group["use"].as_array_mut() {
                for provider in used.iter().filter_map(Value::as_str) {
                    if let Some(nodes) = expanded.get(provider) {
                        names.extend(nodes.iter().cloned());
                    }
                }
                used.retain(|provider| {
                    !provider
                        .as_str()
                        .is_some_and(|name| expanded.contains_key(name))
                });
                if used.is_empty() {
                    group.as_object_mut().unwrap().remove("use");
                }
            }
            // include-all 会自动包含内嵌节点；仅包含 provider 的组需补齐节点引用。
            if all_providers && !all {
                for (provider_name, _) in &providers {
                    names.extend(expanded[provider_name].iter().cloned());
                }
            }
            if !names.is_empty() && !all {
                let proxies = group
                    .as_object_mut()
                    .unwrap()
                    .entry("proxies")
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .ok_or_else(|| invalid("group proxies must be a list"))?;
                for name in names {
                    if selected(&name, &include, &exclude)
                        && !proxies.iter().any(|proxy| proxy.as_str() == Some(&name))
                    {
                        proxies.push(name.into());
                    }
                }
            }
            if all_providers && !has_other_providers {
                group
                    .as_object_mut()
                    .unwrap()
                    .remove("include-all-providers");
            }
            // 没有匹配节点且没有其他动态来源的组沿用 DIRECT 兜底。
            if group.get("use").is_none()
                && !all
                && !(all_providers && has_other_providers)
                && group["include-all-proxies"].as_bool() != Some(true)
                && group["proxies"].as_array().is_none_or(Vec::is_empty)
            {
                group["proxies"] = serde_json::json!(["DIRECT"]);
            }
        }
    }
    let remaining = document["proxy-providers"].as_object_mut().unwrap();
    for (name, _) in providers {
        remaining.remove(&name);
    }
    if remaining.is_empty() {
        document.as_object_mut().unwrap().remove("proxy-providers");
    }
    Ok(())
}
