use crate::{error::AppError, model::DEFAULT_FILENAME};
use axum::{
    Json,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// 同时提供 ASCII 回退名和 RFC 5987 编码名，兼容下载器与中文文件名。
pub fn attachment(base: &str, extension: &str) -> HeaderValue {
    let full = format!("{base}.{extension}");
    let fallback = ascii_fallback(&full);
    let encoded = encode_rfc5987(&full);
    let value = if encoded == fallback {
        format!("attachment; filename=\"{fallback}\"")
    } else {
        format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
    };
    HeaderValue::from_str(&value)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"sub-forge.yaml\""))
}

fn ascii_fallback(name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    match fallback.as_str() {
        "" | "." | ".." => DEFAULT_FILENAME.to_owned(),
        _ => fallback,
    }
}

fn encode_rfc5987(name: &str) -> String {
    let mut encoded = String::with_capacity(name.len());
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
        {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
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

#[cfg(test)]
mod tests {
    use super::attachment;

    #[test]
    fn ascii_filename_uses_plain_disposition() {
        assert_eq!(
            attachment("sub-forge", "yaml"),
            "attachment; filename=\"sub-forge.yaml\""
        );
    }

    #[test]
    fn unicode_filename_adds_rfc5987_form() {
        let value = attachment("我的订阅", "yaml");
        let value = value.to_str().unwrap();
        assert!(value.contains("filename=\"____.yaml\""));
        assert!(value.contains("filename*=UTF-8''%E6%88%91%E7%9A%84%E8%AE%A2%E9%98%85.yaml"));
    }

    #[test]
    fn reserved_characters_are_encoded_and_sanitized() {
        let value = attachment("my sub?#", "txt");
        let value = value.to_str().unwrap();
        assert!(value.contains("filename=\"my_sub__.txt\""));
        assert!(value.contains("filename*=UTF-8''my%20sub%3F#.txt"));
    }
}
