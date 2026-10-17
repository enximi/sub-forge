use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rquickjs::{CatchResultExt, Context, Function, Runtime};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::{config::RuntimeConfig, error::AppError};

#[derive(Clone)]
pub struct ScriptEngine {
    timeout: Duration,
    memory_bytes: usize,
    max_source_bytes: usize,
    slots: Arc<Semaphore>,
}

impl ScriptEngine {
    pub fn new(config: &RuntimeConfig) -> Self {
        Self {
            timeout: config.script_timeout,
            memory_bytes: config.script_memory_bytes,
            max_source_bytes: config.script_max_bytes,
            slots: Arc::new(Semaphore::new(config.script_concurrency)),
        }
    }

    pub async fn execute_source(
        &self,
        source: &str,
        stage: &'static str,
        input: Value,
        mut context: Value,
        max_output_bytes: usize,
    ) -> Result<Value, AppError> {
        context["stage"] = stage.into();
        if source.len() > self.max_source_bytes {
            return Err(AppError::BadRequest("脚本超出服务大小限制".into()));
        }
        let source = source.to_owned();
        let engine = self.clone();
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| AppError::Worker(error.to_string()))?;
        tokio::task::spawn_blocking(move || {
            // 请求取消时线程仍持有许可，防止用户代码绕过执行并发限制。
            let _permit = permit;
            engine.execute(source, stage, input, context, max_output_bytes)
        })
        .await
        .map_err(|error| AppError::Worker(error.to_string()))?
    }

    fn execute(
        &self,
        source: String,
        stage: &'static str,
        input: Value,
        context: Value,
        max_output_bytes: usize,
    ) -> Result<Value, AppError> {
        let failure = |message: String| AppError::Script {
            stage,
            message: message.chars().take(512).collect(),
        };
        let runtime = Runtime::new().map_err(|error| failure(error.to_string()))?;
        runtime.set_memory_limit(self.memory_bytes);
        runtime.set_max_stack_size(1024 * 1024);
        let started = Instant::now();
        let timeout = self.timeout;
        runtime.set_interrupt_handler(Some(Box::new(move || started.elapsed() >= timeout)));
        let js = Context::full(&runtime).map_err(|error| failure(error.to_string()))?;
        js.with(|ctx| {
            // 每个脚本在独立环境中运行，业务扩展不获得文件系统、网络或环境变量访问。
            let wrapper = format!("(function() {{\n{source}\nif (typeof transform !== 'function') throw new Error('script must define transform(input, context)');\nif (transform.constructor?.name === 'AsyncFunction') throw new Error('transform must be synchronous');\nreturn function(input, context) {{\nconst result = transform(input, context);\nif (result && typeof result.then === 'function') throw new Error('transform must be synchronous');\nreturn result;\n}};\n}})()");
            let function: Function = ctx.eval(wrapper).catch(&ctx)
                .map_err(|error| failure(error.to_string()))?;
            let input = ctx.json_parse(input.to_string()).catch(&ctx)
                .map_err(|error| failure(error.to_string()))?;
            let context = ctx.json_parse(context.to_string()).catch(&ctx)
                .map_err(|error| failure(error.to_string()))?;
            let result: rquickjs::Value = function.call((input, context)).catch(&ctx)
                .map_err(|error| failure(error.to_string()))?;
            let json = ctx.json_stringify(result).catch(&ctx)
                .map_err(|error| failure(error.to_string()))?
                .ok_or_else(|| failure("transform must return a JSON-compatible value".into()))?
                .to_string().map_err(|error| failure(error.to_string()))?;
            // 字符串的 JSON 表示包含转义，实际文本上限由对应业务阶段再次检查。
            if json.len() > max_output_bytes.saturating_mul(6).saturating_add(2) {
                return Err(AppError::ResponseTooLarge);
            }
            serde_json::from_str(&json).map_err(|error| failure(error.to_string()))
        })
    }
}
