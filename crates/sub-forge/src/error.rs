use thiserror::Error;

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to load configuration: {0}")]
    Config(#[from] config::ConfigError),
    #[error("failed to load .env: {0}")]
    Dotenv(#[from] dotenvy::Error),
    #[error("invalid configuration {key}: {error}")]
    InvalidEnv { key: &'static str, error: String },
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("请求过于频繁，请一分钟后重试")]
    RateLimited,
    #[error("服务正在处理其他请求，请稍后重试")]
    Busy,
    #[error("服务暂时无法完成请求")]
    Internal,
    #[error("invalid subscription token")]
    Unauthorized,
    #[error("生成 token 缺失或无效")]
    AccessDenied,
    #[error("upstream request failed: {0}")]
    UpstreamRequest(reqwest::Error),
    #[error("upstream response exceeds configured size limit")]
    ResponseTooLarge,
    #[error("generated Mihomo configuration is invalid: {0}")]
    InvalidGeneratedConfig(String),
    #[error("upstream ACL4SSR response is not valid UTF-8")]
    InvalidEncoding,
    #[error("invalid policy: {0}")]
    Policy(String),
    #[error("script {stage} failed: {message}")]
    Script {
        stage: &'static str,
        message: String,
    },
    #[error("background worker failed: {0}")]
    Worker(String),
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        // 上游 URL 可能带有订阅凭据，不把它返回给调用方。
        Self::UpstreamRequest(error.without_url())
    }
}
