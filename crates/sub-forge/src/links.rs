use reqwest::Url;
use serde::Serialize;

use crate::{
    error::AppError,
    model::{LinkKind, SourceConfig, StoredLink, SubscriptionConfig},
    store::Store,
};

pub const API_PREFIX: &str = "/api/v1";

#[derive(Serialize)]
pub struct SavedLink {
    pub id: String,
    pub url: String,
}

#[derive(Clone)]
pub struct Links {
    store: Store,
    base: Url,
}

impl Links {
    pub fn new(store: Store, base: Url) -> Self {
        Self { store, base }
    }

    pub fn url(&self, kind: LinkKind, id: &str) -> Url {
        let mut url = self.base.clone();
        // 路由和链接共用版本前缀，保留反向代理的部署路径。
        url.set_path(&format!(
            "{}{API_PREFIX}/{}/{id}",
            self.base.path().trim_end_matches('/'),
            kind.endpoint()
        ));
        url.set_query(None);
        url.set_fragment(None);
        url
    }

    pub async fn save(&self, record: StoredLink) -> Result<SavedLink, AppError> {
        let kind = record.kind();
        let id = self.store.save(record).await?;
        Ok(SavedLink {
            url: self.url(kind, &id).to_string(),
            id,
        })
    }

    pub async fn feed(&self, id: &str) -> Result<SubscriptionConfig, AppError> {
        match self.store.load(id).await? {
            StoredLink::Feed(config) => Ok(config),
            _ => Err(AppError::Unauthorized),
        }
    }

    pub async fn source(&self, id: &str, kind: LinkKind) -> Result<SourceConfig, AppError> {
        match (kind, self.store.load(id).await?) {
            (LinkKind::Subscription, StoredLink::Subscription(config))
            | (LinkKind::Rules, StoredLink::Rules(config)) => Ok(config),
            _ => Err(AppError::Unauthorized),
        }
    }

    pub async fn resource(&self, id: &str) -> Result<String, AppError> {
        match self.store.load(id).await? {
            StoredLink::RuleResource { source_url } => Ok(source_url),
            _ => Err(AppError::Unauthorized),
        }
    }
}
