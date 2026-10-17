use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{ConnectInfo, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    api,
    config::RuntimeConfig,
    converter::{ConversionRequest, Converter},
    error::AppError,
    limits::RequestLimits,
    network::PublicNetwork,
    pipeline::ConfigPipeline,
    policy::Policy,
    rules::{RuleSource, template_marker, text_result},
    scripts::ScriptEngine,
    subscription::{self, SubscriptionSource},
    tickets::{self, Scope, Ticket},
};

pub struct AppState {
    public_base_url: reqwest::Url,
    access_token: Option<String>,
    limits: RequestLimits,
    allowed_origins: Vec<HeaderValue>,
    conversions: Semaphore,
    fetches: Semaphore,
    timeout: Duration,
    scripts: ScriptEngine,
    rules: RuleSource,
    subscriptions: SubscriptionSource,
    pipeline: ConfigPipeline,
    max_subscription_bytes: usize,
}

impl AppState {
    pub fn from_config(config: RuntimeConfig) -> Self {
        let rules_network = PublicNetwork::new(config.rules_timeout);
        let network = PublicNetwork::new(config.request_timeout);
        let converter = Converter::new(network.clone(), config.max_config_bytes);
        let scripts = ScriptEngine::new(&config);
        Self {
            access_token: config.access_token,
            allowed_origins: config.allowed_origins,
            limits: RequestLimits::new(config.requests_per_minute, config.trusted_proxies),
            conversions: Semaphore::new(config.conversion_concurrency),
            fetches: Semaphore::new(config.fetch_concurrency),
            timeout: config.request_timeout,
            public_base_url: config.public_base_url,
            scripts,
            rules: RuleSource::new(rules_network, config.max_rules_bytes),
            subscriptions: SubscriptionSource::new(network, config.max_subscription_bytes),
            pipeline: ConfigPipeline::new(converter, config.max_config_bytes),
            max_subscription_bytes: config.max_subscription_bytes,
        }
    }

    fn conversion_request(
        &self,
        ticket: &Ticket,
        policy: &Policy,
    ) -> Result<ConversionRequest, AppError> {
        let source =
            PublicNetwork::parse(ticket.source_url.as_deref().ok_or(AppError::Unauthorized)?)?;
        let converter_url = PublicNetwork::parse(
            ticket
                .converter_url
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("请提供转换器地址并重新生成订阅".into()))?,
        )?;
        let scripts = ticket.scripts.as_ref().ok_or(AppError::Unauthorized)?;
        let subscription_token = tickets::encode(&Ticket {
            scope: Scope::Subscription,
            source_url: Some(source.to_string()),
            converter_url: None,
            scripts: None,
            policy: None,
            script: Some(scripts.subscription.clone()),
        })?;
        let rules_token = tickets::encode(&Ticket {
            scope: Scope::Rules,
            source_url: Some(policy.rules.source_url.clone()),
            converter_url: None,
            scripts: None,
            policy: None,
            script: Some(scripts.preprocess_rules.clone()),
        })?;
        let base = &self.public_base_url;
        Ok(ConversionRequest {
            converter_url,
            options: policy.converter.clone(),
            subscription_url: subscription::public_url(
                base,
                &subscription_token,
                self.access_token.as_deref(),
            ),
            rules_url: api::url(
                base,
                &format!("rules/{rules_token}"),
                self.access_token.as_deref(),
            ),
            rules_marker: template_marker(&rules_token),
        })
    }
}

fn empty_context() -> Value {
    // 不单独暴露凭据字段；私有模式的相关回调地址会携带访问 token。
    json!({ "version": 1, "urls": {}, "secrets": {} })
}

pub fn api_router(state: Arc<AppState>) -> Router {
    // 跨域来源由部署者明确指定；核心不依赖任何特定前端或会话 Cookie。
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(state.allowed_origins.clone()))
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    Router::new()
        .route("/health", get(health))
        .nest(
            api::PREFIX,
            Router::new()
                .route("/clash/{token}", get(clash))
                .route("/subscription/{token}", get(subscription))
                .route("/rules/{token}", get(rules))
                .route("/rule-resource/{token}", get(rule_resource)),
        )
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        // CORS 包裹鉴权，使前端能读取 401，并允许无凭据的预检请求。
        .layer(cors)
        .with_state(state)
}

async fn protect(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let dynamic = request.uri().path().starts_with("/api/");
    let mut response = if dynamic {
        match request.extensions().get::<ConnectInfo<SocketAddr>>() {
            Some(ConnectInfo(peer)) => match state
                .limits
                .admit(*peer, request.headers())
                .and_then(|()| api::authorize(state.access_token.as_deref(), &request))
            {
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
        (
            "content-security-policy",
            "default-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    ] {
        headers.insert(key, HeaderValue::from_static(value));
    }
    response
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn clash(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _permit = state
        .conversions
        .try_acquire()
        .map_err(|_| AppError::Busy)?;
    let ticket = tickets::decode(&token, Scope::Feed)?;
    let scripts = ticket.scripts.as_ref().ok_or(AppError::Unauthorized)?;
    let mut context = empty_context();
    let policy = ticket.policy.as_ref().ok_or(AppError::Unauthorized)?;
    policy.validate()?;
    let request = state.conversion_request(&ticket, policy)?;
    context["urls"] = json!({ "subscription": request.subscription_url.as_str(), "rules": request.rules_url.as_str() });
    let content = tokio::time::timeout(
        state.timeout,
        state.pipeline.generate(
            &scripts.postprocess_config,
            &state.scripts,
            context,
            &request,
        ),
    )
    .await
    .map_err(|_| AppError::Busy)??;
    Ok((
        [
            (header::CONTENT_TYPE, "application/yaml; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"sub-forge.yaml\"",
            ),
        ],
        content,
    ))
}

async fn subscription(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _permit = state.fetches.try_acquire().map_err(|_| AppError::Busy)?;
    let ticket = tickets::decode(&token, Scope::Subscription)?;
    let source = PublicNetwork::parse(ticket.source_url.as_deref().ok_or(AppError::Unauthorized)?)?;
    let mut context = empty_context();
    context["urls"]["subscription"] = subscription::public_url(
        &state.public_base_url,
        &token,
        state.access_token.as_deref(),
    )
    .as_str()
    .into();
    let (mut content, upstream_headers) = state.subscriptions.fetch(source).await?;
    let script = ticket.script.as_deref().ok_or(AppError::Unauthorized)?;
    if !script.trim().is_empty() {
        let source = String::from_utf8(content).map_err(|_| AppError::InvalidEncoding)?;
        let result = state
            .scripts
            .execute_source(
                script,
                "subscription",
                source.into(),
                context,
                state.max_subscription_bytes,
            )
            .await?;
        content = text_result(result, state.max_subscription_bytes)?.into_bytes();
    }
    // 上游内容不作为本站 HTML/JavaScript 执行，仅转发订阅用量信息。
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"subscription.txt\""),
    );
    if let Some(value) = upstream_headers.get("subscription-userinfo") {
        headers.insert("subscription-userinfo", value.clone());
    }
    Ok((headers, content))
}

async fn rules(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _permit = state.fetches.try_acquire().map_err(|_| AppError::Busy)?;
    let ticket = tickets::decode(&token, Scope::Rules)?;
    let source_url = ticket.source_url.as_deref().ok_or(AppError::Unauthorized)?;
    let script = ticket.script.as_deref().ok_or(AppError::Unauthorized)?;
    let mut context = empty_context();
    context["urls"]["rules"] = api::url(
        &state.public_base_url,
        &format!("rules/{token}"),
        state.access_token.as_deref(),
    )
    .as_str()
    .into();
    let mut content = state
        .rules
        .config(
            source_url,
            script,
            &state.scripts,
            context,
            &state.public_base_url,
            state.access_token.as_deref(),
        )
        .await?;
    // 转换器取模板失败时会静默退回默认配置，用内容标记确认本次模板已被采用。
    content.push_str(&format!(
        "custom_proxy_group={}`select`[]DIRECT\n",
        template_marker(&token)
    ));
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        content,
    ))
}

async fn rule_resource(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _permit = state.fetches.try_acquire().map_err(|_| AppError::Busy)?;
    let ticket = tickets::decode(&token, Scope::RuleResource)?;
    let url = PublicNetwork::parse(ticket.source_url.as_deref().ok_or(AppError::Unauthorized)?)?;
    let content = state.rules.resource(url).await?;
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        content,
    ))
}
