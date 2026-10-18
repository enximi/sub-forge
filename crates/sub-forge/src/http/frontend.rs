use axum::{
    Router,
    extract::State,
    http::{HeaderValue, header},
    response::IntoResponse,
    routing::get,
};

use super::HttpState;
use std::sync::Arc;

const PAGE: &str = include_str!("../../../../frontend/index.html");
const STYLES: &str = include_str!("../../../../frontend/styles.css");
const APP: &str = include_str!("../../../../frontend/app.js");
const API: &str = include_str!("../../../../frontend/api.js");
const RULES_PICKER: &str = include_str!("../../../../frontend/rules-picker.js");

pub(super) fn routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route(
            "/styles.css",
            get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES) }),
        )
        .route("/app.js", get(|| javascript(APP)))
        .route("/api.js", get(|| javascript(API)))
        .route("/rules-picker.js", get(|| javascript(RULES_PICKER)))
}

async fn index(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    // 前端仅加载内置资源、访问同源 API；用户脚本始终作为文本提交。
    let page = page(state.access_token.is_some());
    let mut response = ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response();
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
    ));
    // 页面内容随部署鉴权配置变化，避免浏览器复用旧的 token 显示状态。
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn page(access_token_required: bool) -> String {
    PAGE.replace(
        "__ACCESS_TOKEN_REQUIRED__",
        if access_token_required {
            "true"
        } else {
            "false"
        },
    )
}

async fn javascript(source: &'static str) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        source,
    )
}

#[cfg(test)]
mod tests {
    use super::page;

    #[test]
    fn page_marks_access_token_requirement() {
        assert!(page(true).contains("data-access-token-required=\"true\""));
        assert!(page(false).contains("data-access-token-required=\"false\""));
        assert!(!page(false).contains("__ACCESS_TOKEN_REQUIRED__"));
    }
}
