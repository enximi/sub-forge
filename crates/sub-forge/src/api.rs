use axum::{
    extract::{Query, Request},
    http::header,
};
use reqwest::Url;
use serde::Deserialize;
use subtle::ConstantTimeEq;

use crate::error::AppError;

pub const PREFIX: &str = "/api/v1";

pub fn url(base: &Url, endpoint: &str, access_token: Option<&str>) -> Url {
    // 路由与生成链接共用版本前缀，保留部署地址已有的路径前缀。
    let mut url = base.clone();
    url.set_path(&format!(
        "{}{PREFIX}/{endpoint}",
        url.path().trim_end_matches('/')
    ));
    url.set_query(None);
    url.set_fragment(None);
    if let Some(token) = access_token {
        url.query_pairs_mut().append_pair("access_token", token);
    }
    url
}

#[derive(Deserialize)]
struct AccessQuery {
    access_token: Option<String>,
}

pub fn authorize(expected: Option<&str>, request: &Request) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let Query(query) =
        Query::<AccessQuery>::try_from_uri(request.uri()).map_err(|_| AppError::AccessDenied)?;
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
        .transpose()?;
    // 重复参数、重复请求头及相互冲突的凭据均拒绝，避免代理与应用解释不一致。
    if credentials.next().is_some() {
        return Err(AppError::AccessDenied);
    }
    let supplied = [query.access_token.as_deref(), bearer];
    if supplied.iter().all(Option::is_none)
        || supplied
            .into_iter()
            .flatten()
            .any(|token| !bool::from(expected.as_bytes().ct_eq(token.as_bytes())))
    {
        return Err(AppError::AccessDenied);
    }
    Ok(())
}
