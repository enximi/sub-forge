mod frontend;
mod handlers;
mod middleware;
mod response;

use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{Method, header},
    routing::{get, post},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    config::RuntimeConfig, limits::RequestLimits, links::API_PREFIX, service::SubscriptionService,
};

struct HttpState {
    service: SubscriptionService,
    access_token: Option<String>,
    limits: RequestLimits,
}

pub fn router(config: &RuntimeConfig, service: SubscriptionService) -> Router {
    let state = Arc::new(HttpState {
        service,
        access_token: config.access_token.clone(),
        limits: RequestLimits::new(config.requests_per_minute, config.trusted_proxies.clone()),
    });
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(config.allowed_origins.clone()))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    frontend::routes()
        .route("/health", get(handlers::health))
        .nest(
            API_PREFIX,
            Router::new()
                .route(
                    "/subscriptions",
                    post(handlers::create).route_layer(axum::middleware::from_fn_with_state(
                        state.clone(),
                        middleware::authorize_generation,
                    )),
                )
                .route("/clash/{id}", get(handlers::clash))
                .route("/subscription/{id}", get(handlers::subscription))
                .route("/rules/{id}", get(handlers::rules))
                .route("/rule-resource/{id}", get(handlers::rule_resource))
                .layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::protect,
        ))
        // CORS 包裹鉴权，使浏览器能读取 401，并允许无凭据的预检请求。
        .layer(cors)
        .with_state(state)
}
