use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;
use ipnet::IpNet;

use crate::error::AppError;

pub struct RequestLimits {
    clients: Mutex<HashMap<IpAddr, (Instant, u32)>>,
    per_minute: u32,
    trusted_proxies: Vec<IpNet>,
}

impl RequestLimits {
    pub fn new(per_minute: u32, trusted_proxies: Vec<IpNet>) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            per_minute,
            trusted_proxies,
        }
    }

    pub fn admit(&self, peer: SocketAddr, headers: &HeaderMap) -> Result<(), AppError> {
        let trusted = |ip: IpAddr| self.trusted_proxies.iter().any(|range| range.contains(&ip));
        let mut ip = peer.ip();
        // 只有管理员声明的代理可以提供转发地址；从右向左剥离可信代理。
        if trusted(ip)
            && let Some(chain) = headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
            && let Ok(chain) = chain
                .split(',')
                .map(|value| value.trim().parse::<IpAddr>())
                .collect::<Result<Vec<_>, _>>()
        {
            for address in chain.into_iter().rev() {
                if !trusted(ip) {
                    break;
                }
                ip = address;
            }
        }
        let now = Instant::now();
        let window = Duration::from_secs(60);
        let mut clients = self.clients.lock().map_err(|_| AppError::Internal)?;
        if clients.len() >= 10000 {
            clients.retain(|_, (started, _)| now.duration_since(*started) < window);
            if clients.len() >= 10000 && !clients.contains_key(&ip) {
                return Err(AppError::RateLimited);
            }
        }
        let (started, count) = clients.entry(ip).or_insert((now, 0));
        if now.duration_since(*started) >= window {
            *started = now;
            *count = 0;
        }
        if *count >= self.per_minute {
            return Err(AppError::RateLimited);
        }
        *count += 1;
        Ok(())
    }
}
