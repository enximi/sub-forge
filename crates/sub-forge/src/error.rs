use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
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
    #[error("访问 token 缺失或无效")]
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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::UpstreamRequest(error) = &self {
            // 只记录错误分类，不记录 URL、响应内容、个人设置或登录凭据。
            tracing::warn!(
                timeout = error.is_timeout(),
                connect = error.is_connect(),
                status = error.status().map(|value| value.as_u16()),
                "上游请求失败"
            );
        }
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Busy => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unauthorized | Self::AccessDenied => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_GATEWAY,
        };
        let message = match &self {
            Self::BadRequest(message) => message.clone(),
            Self::Unauthorized => "订阅地址无效，请重新生成".into(),
            Self::RateLimited | Self::Busy | Self::Internal | Self::AccessDenied => {
                self.to_string()
            }
            Self::Script { stage, message } => format!("{stage} 脚本执行失败：{message}"),
            Self::ResponseTooLarge => "订阅或生成的配置超出服务大小限制".into(),
            _ => "无法完成转换，请检查订阅链接和转换器是否可用".into(),
        };
        let mut response = (status, Json(ErrorBody { error: message })).into_response();
        if matches!(self, Self::AccessDenied) {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer realm=\"SubForge\""),
            );
        }
        if matches!(self, Self::RateLimited | Self::Busy) {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("60"));
        }
        response
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        // 上游 URL 可能带有订阅凭据，不把它返回给调用方。
        Self::UpstreamRequest(error.without_url())
    }
}
