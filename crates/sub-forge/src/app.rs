use tokio::net::TcpListener;
use tracing::info;

use crate::{
    config::RuntimeConfig, http, links::Links, service::SubscriptionService, store::Store,
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
    let store = Store::open(&config.database_path)?;
    let links = Links::new(store, config.public_base_url.clone());
    let service = SubscriptionService::new(&config, links);
    let public = TcpListener::bind(listen_addr).await?;
    info!(public = %public.local_addr()?, "sub-forge listener");
    axum::serve(
        public,
        http::router(&config, service)
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
