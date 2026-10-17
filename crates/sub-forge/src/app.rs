use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::info;

use crate::{
    config::RuntimeConfig,
    http::{AppState, api_router},
};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sub_forge=info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let config = RuntimeConfig::load()?;
    let listen_addr = config.listen_addr;
    let state = Arc::new(AppState::from_config(config));
    let public = TcpListener::bind(listen_addr).await?;
    info!(public = %public.local_addr()?, "sub-forge listener");
    axum::serve(
        public,
        api_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
