use crate::error::AppError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
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
