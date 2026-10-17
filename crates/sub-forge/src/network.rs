use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use reqwest::{Client, Response, Url, header::LOCATION, redirect};

use crate::error::AppError;

#[derive(Clone)]
pub struct PublicNetwork {
    timeout: Duration,
}

impl PublicNetwork {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn parse(raw: &str) -> Result<Url, AppError> {
        if raw.len() > 2048 {
            return Err(AppError::BadRequest("链接过长，最多支持 2048 字节".into()));
        }
        let mut url = Url::parse(raw.trim())
            .map_err(|_| AppError::BadRequest("请输入完整的 HTTP 或 HTTPS 链接".into()))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(AppError::BadRequest(
                "只支持不含用户名和密码的 HTTP 或 HTTPS 链接".into(),
            ));
        }
        url.set_fragment(None);
        if url.as_str().len() > 2048 {
            return Err(AppError::BadRequest(
                "链接规范化后过长，最多支持 2048 字节".into(),
            ));
        }
        Ok(url)
    }

    pub async fn validate(&self, url: &Url) -> Result<(), AppError> {
        self.addresses(url).await.map(|_| ())
    }

    async fn addresses(&self, url: &Url) -> Result<(String, Vec<SocketAddr>), AppError> {
        // 每次请求、每次跳转重新解析并校验；连接只使用这次已校验的地址。
        let host = url
            .host_str()
            .ok_or_else(blocked)?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let port = url.port_or_known_default().ok_or_else(blocked)?;
        let mut addresses = if let Ok(ip) = host.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, port)]
        } else {
            tokio::time::timeout(self.timeout, tokio::net::lookup_host((host.as_str(), port)))
                .await
                .map_err(|_| blocked())?
                .map_err(|_| blocked())?
                .collect::<Vec<_>>()
        };
        if addresses.is_empty() || addresses.iter().any(|address| !is_public(address.ip())) {
            return Err(blocked());
        }
        addresses.sort_unstable();
        addresses.dedup();
        Ok((host, addresses))
    }

    pub async fn get(&self, url: Url) -> Result<Response, AppError> {
        tokio::time::timeout(self.timeout, self.get_redirects(url))
            .await
            .map_err(|_| AppError::Busy)?
    }

    async fn get_redirects(&self, mut url: Url) -> Result<Response, AppError> {
        for hop in 0..=5 {
            let (host, addresses) = self.addresses(&url).await?;
            let client = Client::builder()
                .no_proxy()
                .redirect(redirect::Policy::none())
                .timeout(self.timeout)
                .resolve_to_addrs(&host, &addresses)
                .build()?;
            let response = client.get(url.clone()).send().await?;
            if response.status().is_redirection() {
                if hop == 5 {
                    return Err(AppError::BadRequest("上游链接跳转次数过多".into()));
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| AppError::BadRequest("上游返回了无效的跳转地址".into()))?;
                url = Self::parse(url.join(location).map_err(|_| blocked())?.as_str())?;
                continue;
            }
            return Ok(response.error_for_status()?);
        }
        Err(AppError::Internal)
    }
}

fn blocked() -> AppError {
    AppError::BadRequest("链接必须指向可解析的公网地址，不能访问本机、内网或保留地址".into())
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let blocked_ranges = [
                "0.0.0.0/8",
                "10.0.0.0/8",
                "100.64.0.0/10",
                "127.0.0.0/8",
                "169.254.0.0/16",
                "172.16.0.0/12",
                "192.0.0.0/24",
                "192.0.2.0/24",
                "192.88.99.0/24",
                "192.168.0.0/16",
                "198.18.0.0/15",
                "198.51.100.0/24",
                "203.0.113.0/24",
                "224.0.0.0/4",
                "240.0.0.0/4",
            ];
            !blocked_ranges
                .iter()
                .any(|range| range.parse::<ipnet::Ipv4Net>().unwrap().contains(&ip))
        }
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return is_public(IpAddr::V4(ip));
            }
            "2000::/3".parse::<ipnet::Ipv6Net>().unwrap().contains(&ip)
                && !["2001::/23", "2001:db8::/32", "2002::/16", "3fff::/20"]
                    .iter()
                    .any(|range| range.parse::<ipnet::Ipv6Net>().unwrap().contains(&ip))
        }
    }
}
