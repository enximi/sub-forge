use axum::{
    Router,
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

async fn index() -> impl IntoResponse {
    // 前端仅加载内置资源、访问同源 API；用户脚本始终作为文本提交。
    let mut response = ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], PAGE).into_response();
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
    ));
    response
}

async fn javascript(source: &'static str) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        source,
    )
}
