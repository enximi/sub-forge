use serde_json::Value;

use crate::{
    converter::{ConversionRequest, Converter},
    error::AppError,
    scripts::ScriptEngine,
};

#[derive(Clone)]
pub struct ConfigPipeline {
    converter: Converter,
    max_bytes: usize,
}

impl ConfigPipeline {
    pub fn new(converter: Converter, max_bytes: usize) -> Self {
        Self {
            converter,
            max_bytes,
        }
    }

    pub async fn generate(
        &self,
        postprocess_script: &str,
        scripts: &ScriptEngine,
        context: Value,
        request: &ConversionRequest,
    ) -> Result<Vec<u8>, AppError> {
        let content = self.converter.generate(request).await?;
        let mut document: Value = serde_yaml_ng::from_slice(&content)
            .map_err(|error| AppError::InvalidGeneratedConfig(error.to_string()))?;
        if !document.is_object() {
            return Err(AppError::InvalidGeneratedConfig(
                "top-level YAML must be a mapping".into(),
            ));
        }
        let groups = document
            .get_mut("proxy-groups")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                AppError::InvalidGeneratedConfig("rules callback was not applied".into())
            })?;
        let marker = groups
            .iter()
            .position(|group| {
                group.get("name").and_then(Value::as_str) == Some(&request.rules_marker)
            })
            .ok_or_else(|| {
                AppError::InvalidGeneratedConfig("rules callback was not applied".into())
            })?;
        groups.remove(marker);
        if let Some(providers) = document
            .get_mut("proxy-providers")
            .and_then(Value::as_object_mut)
        {
            for provider in providers.values_mut() {
                if provider.get("url").and_then(Value::as_str)
                    == Some(request.subscription_url.as_str())
                {
                    // 部分转换器忽略 interval 参数，本站保证本次订阅的刷新间隔生效。
                    provider["interval"] = request.options.interval_secs.into();
                }
            }
        }
        let document = if postprocess_script.trim().is_empty() {
            document
        } else {
            scripts
                .execute_source(
                    postprocess_script,
                    "postprocess_config",
                    document,
                    context,
                    self.max_bytes,
                )
                .await?
        };
        if !document.is_object() {
            return Err(AppError::InvalidGeneratedConfig(
                "config script must return an object".into(),
            ));
        }
        let content = serde_yaml_ng::to_string(&document)
            .map_err(|error| AppError::InvalidGeneratedConfig(error.to_string()))?
            .into_bytes();
        if content.len() > self.max_bytes {
            return Err(AppError::ResponseTooLarge);
        }
        Ok(content)
    }
}
