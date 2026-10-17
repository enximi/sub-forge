use axum::http::HeaderMap;
use reqwest::Url;

use crate::{api, error::AppError, network::PublicNetwork, upstream::read_limited_response};

#[derive(Clone)]
pub struct SubscriptionSource {
    network: PublicNetwork,
    max_bytes: usize,
}

impl SubscriptionSource {
    pub fn new(network: PublicNetwork, max_bytes: usize) -> Self {
        Self { network, max_bytes }
    }

    pub async fn fetch(&self, url: Url) -> Result<(Vec<u8>, HeaderMap), AppError> {
        let response = self.network.get(url).await?;
        let headers = response.headers().clone();
        let content = read_limited_response(response, self.max_bytes).await?;
        Ok((content, headers))
    }
}

pub fn public_url(base: &Url, token: &str, access_token: Option<&str>) -> Url {
    api::url(base, &format!("subscription/{token}"), access_token)
}
