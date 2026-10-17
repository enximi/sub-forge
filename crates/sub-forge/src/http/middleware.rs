use super::HttpState;
use crate::error::AppError;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{net::SocketAddr, sync::Arc};
use subtle::ConstantTimeEq;

pub(super) async fn protect(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let dynamic = request.uri().path().starts_with("/api/");
    let mut response = if dynamic {
        match request.extensions().get::<ConnectInfo<SocketAddr>>() {
            Some(ConnectInfo(peer)) => match state.limits.admit(*peer, request.headers()) {
                Ok(()) => next.run(request).await,
                Err(error) => error.into_response(),
            },
            None => AppError::Internal.into_response(),
        }
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    for (key, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
    ] {
        headers.insert(key, HeaderValue::from_static(value));
    }
    headers
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ));
    response
}

pub(super) async fn authorize_generation(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    match authorize(state.access_token.as_deref(), &request) {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}

fn authorize(expected: Option<&str>, request: &Request) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let mut credentials = request.headers().get_all(header::AUTHORIZATION).iter();
    let bearer = credentials
        .next()
        .map(|header| {
            let (scheme, token) = header
                .to_str()
                .ok()
                .and_then(|value| value.split_once(' '))
                .ok_or(AppError::AccessDenied)?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token)
                .ok_or(AppError::AccessDenied)
        })
        .transpose()?
        .ok_or(AppError::AccessDenied)?;
    // 仅接受一个 Bearer 请求头，生成凭据不从 URL 读取。
    if credentials.next().is_some() || !bool::from(expected.as_bytes().ct_eq(bearer.as_bytes())) {
        return Err(AppError::AccessDenied);
    }
    Ok(())
}
