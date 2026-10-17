use std::{net::SocketAddr, time::Duration};

use config::{Config, Environment};
use reqwest::Url;
use serde::Deserialize;

use crate::error::StartupError;

pub struct RuntimeConfig {
    pub public_base_url: Url,
    pub access_token: Option<String>,
    pub listen_addr: SocketAddr,
    pub trusted_proxies: Vec<ipnet::IpNet>,
    pub allowed_origins: Vec<axum::http::HeaderValue>,
    pub requests_per_minute: u32,
    pub conversion_concurrency: usize,
    pub fetch_concurrency: usize,
    pub request_timeout: Duration,
    pub rules_timeout: Duration,
    pub max_config_bytes: usize,
    pub max_subscription_bytes: usize,
    pub max_rules_bytes: usize,
    pub script_timeout: Duration,
    pub script_memory_bytes: usize,
    pub script_max_bytes: usize,
    pub script_concurrency: usize,
}

#[derive(Deserialize)]
struct EnvironmentConfig {
    sub_forge_public_base_url: String,
    sub_forge_access_token: String,
    sub_forge_listen_addr: String,
    trusted_proxy_cidrs: String,
    sub_forge_allowed_origins: String,
    requests_per_minute: u32,
    conversion_concurrency: usize,
    fetch_concurrency: usize,
    sub_forge_timeout_secs: u64,
    sub_forge_rules_timeout_secs: u64,
    max_generated_config_bytes: usize,
    max_subscription_bytes: usize,
    max_acl4ssr_config_bytes: usize,
    script_timeout_ms: u64,
    script_memory_bytes: usize,
    script_max_bytes: usize,
    script_concurrency: usize,
}

impl RuntimeConfig {
    pub fn load() -> Result<Self, StartupError> {
        match dotenvy::dotenv() {
            Ok(_) => {}
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let input: EnvironmentConfig = Config::builder()
            .set_default("sub_forge_access_token", "")?
            .set_default("trusted_proxy_cidrs", "")?
            .set_default("sub_forge_allowed_origins", "")?
            .set_default("requests_per_minute", 60)?
            .set_default("conversion_concurrency", 4)?
            .set_default("fetch_concurrency", 16)?
            .set_default("sub_forge_listen_addr", "0.0.0.0:8080")?
            .set_default("sub_forge_timeout_secs", 60)?
            .set_default("sub_forge_rules_timeout_secs", 30)?
            .set_default("max_generated_config_bytes", 20 * 1024 * 1024)?
            .set_default("max_subscription_bytes", 10 * 1024 * 1024)?
            .set_default("max_acl4ssr_config_bytes", 2 * 1024 * 1024)?
            .set_default("script_timeout_ms", 2000)?
            .set_default("script_memory_bytes", 128 * 1024 * 1024)?
            .set_default("script_max_bytes", 1024 * 1024)?
            .set_default("script_concurrency", 4)?
            .add_source(Environment::default().ignore_empty(true).try_parsing(true))
            .build()?
            .try_deserialize()?;

        Ok(Self {
            access_token: access_token(input.sub_forge_access_token)?,
            allowed_origins: allowed_origins(&input.sub_forge_allowed_origins)?,
            trusted_proxies: input
                .trusted_proxy_cidrs
                .split(',')
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    value.trim().parse::<ipnet::IpNet>().map_err(|_| {
                        invalid("TRUSTED_PROXY_CIDRS", "must be comma-separated IP networks")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            requests_per_minute: positive(input.requests_per_minute, "REQUESTS_PER_MINUTE")?,
            conversion_concurrency: positive(
                input.conversion_concurrency,
                "CONVERSION_CONCURRENCY",
            )?,
            fetch_concurrency: positive(input.fetch_concurrency, "FETCH_CONCURRENCY")?,
            public_base_url: http_url(
                &input.sub_forge_public_base_url,
                "SUB_FORGE_PUBLIC_BASE_URL",
            )?,
            listen_addr: socket_addr(&input.sub_forge_listen_addr, "SUB_FORGE_LISTEN_ADDR")?,
            request_timeout: Duration::from_secs(positive(
                input.sub_forge_timeout_secs,
                "SUB_FORGE_TIMEOUT_SECS",
            )?),
            rules_timeout: Duration::from_secs(positive(
                input.sub_forge_rules_timeout_secs,
                "SUB_FORGE_RULES_TIMEOUT_SECS",
            )?),
            max_config_bytes: positive(
                input.max_generated_config_bytes,
                "MAX_GENERATED_CONFIG_BYTES",
            )?,
            max_subscription_bytes: positive(
                input.max_subscription_bytes,
                "MAX_SUBSCRIPTION_BYTES",
            )?,
            max_rules_bytes: positive(input.max_acl4ssr_config_bytes, "MAX_ACL4SSR_CONFIG_BYTES")?,
            script_timeout: Duration::from_millis(positive(
                input.script_timeout_ms,
                "SCRIPT_TIMEOUT_MS",
            )?),
            script_memory_bytes: positive(input.script_memory_bytes, "SCRIPT_MEMORY_BYTES")?,
            script_max_bytes: positive(input.script_max_bytes, "SCRIPT_MAX_BYTES")?,
            script_concurrency: positive(input.script_concurrency, "SCRIPT_CONCURRENCY")?,
        })
    }
}

fn access_token(value: String) -> Result<Option<String>, StartupError> {
    if value.is_empty() {
        return Ok(None);
    }
    if !(32..=256).contains(&value.len()) || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(invalid(
            "SUB_FORGE_ACCESS_TOKEN",
            "must contain 32 to 256 printable ASCII characters without whitespace",
        ));
    }
    Ok(Some(value))
}

pub fn http_url(raw: &str, key: &'static str) -> Result<Url, StartupError> {
    let url = Url::parse(raw).map_err(|_| invalid(key, "must be an absolute HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid(key, "must be an absolute HTTP(S) URL"));
    }
    Ok(url)
}

fn socket_addr(raw: &str, key: &'static str) -> Result<SocketAddr, StartupError> {
    raw.parse()
        .map_err(|_| invalid(key, "must be a socket address"))
}

fn positive<T: Default + PartialEq>(value: T, key: &'static str) -> Result<T, StartupError> {
    if value == T::default() {
        return Err(invalid(key, "must be greater than zero"));
    }
    Ok(value)
}

fn invalid(key: &'static str, error: &str) -> StartupError {
    StartupError::InvalidEnv {
        key,
        error: error.to_owned(),
    }
}

fn allowed_origins(raw: &str) -> Result<Vec<axum::http::HeaderValue>, StartupError> {
    const KEY: &str = "SUB_FORGE_ALLOWED_ORIGINS";
    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            let url = http_url(origin, KEY)?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(invalid(KEY, "must contain only explicit HTTP(S) origins"));
            }
            // 浏览器 Origin 不包含末尾斜杠，统一为协议、主机和端口。
            axum::http::HeaderValue::from_str(&url.origin().ascii_serialization())
                .map_err(|_| invalid(KEY, "invalid origin"))
        })
        .collect()
}
