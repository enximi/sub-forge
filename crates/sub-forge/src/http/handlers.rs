use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};

use super::HttpState;
use crate::{error::AppError, model::SubscriptionConfig};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubscriptionLink {
    url: String,
}

pub(super) async fn load(
    State(state): State<Arc<HttpState>>,
    input: Result<Json<SubscriptionLink>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Json(input) = input.map_err(|error| AppError::BadRequest(error.body_text()))?;
    Ok(Json(state.service.load_subscription(&input.url).await?))
}

pub(super) async fn update(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    input: Result<Json<SubscriptionConfig>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Json(input) = input.map_err(|error| AppError::BadRequest(error.body_text()))?;
    Ok(Json(state.service.update(&id, input).await?))
}

pub(super) async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub(super) async fn create(
    State(state): State<Arc<HttpState>>,
    input: Result<Json<SubscriptionConfig>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Json(input) = input.map_err(|error| AppError::BadRequest(error.body_text()))?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.create(input).await?),
    ))
}

pub(super) async fn clash(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok((
        [
            (header::CONTENT_TYPE, "application/yaml; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"sub-forge.yaml\"",
            ),
        ],
        state.service.generate(&id).await?,
    ))
}

pub(super) async fn subscription(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let content = state.service.subscription(&id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"subscription.txt\""),
    );
    // 只转发订阅用量信息，不透传上游的内容类型或其他响应头。
    if let Some(userinfo) = content.userinfo {
        headers.insert("subscription-userinfo", userinfo);
    }
    Ok((headers, content.body))
}

pub(super) async fn rules(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        state.service.rules(&id).await?,
    ))
}

pub(super) async fn rule_resource(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        state.service.rule_resource(&id).await?,
    ))
}
