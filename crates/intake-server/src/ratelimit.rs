//! Token buckets keyed by client address + site (or conversation). In-process: one Fly
//! machine, so no shared store is needed yet.

use std::num::NonZeroU32;

use axum::http::HeaderMap;
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};

pub type KeyedLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

pub struct RateLimiters {
    /// `POST /v1/conversations`, keyed by `ip|site_key`.
    pub start: KeyedLimiter,
    /// `POST /v1/conversations/{id}/messages`, keyed by `ip|conversation_id`.
    pub message: KeyedLimiter,
}

impl RateLimiters {
    pub const PER_MINUTE: u32 = 10;

    pub fn new() -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(Self::PER_MINUTE).expect("non-zero"));
        RateLimiters { start: RateLimiter::keyed(quota), message: RateLimiter::keyed(quota) }
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort client address: proxy headers when trusted, else the peer address the
/// caller passes in, else `"unknown"` (which still rate-limits, just coarsely).
pub fn client_ip(headers: &HeaderMap, peer: Option<std::net::SocketAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(v) = headers.get("fly-client-ip").and_then(|v| v.to_str().ok()) {
            return v.trim().to_string();
        }
        if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    peer.map(|p| p.ip().to_string()).unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleventh_hit_in_a_minute_is_refused() {
        let l = RateLimiters::new();
        for _ in 0..RateLimiters::PER_MINUTE {
            assert!(l.start.check_key(&"1.2.3.4|site".to_string()).is_ok());
        }
        assert!(l.start.check_key(&"1.2.3.4|site".to_string()).is_err());
        assert!(l.start.check_key(&"1.2.3.5|site".to_string()).is_ok(), "other key unaffected");
    }

    #[test]
    fn proxy_headers_only_when_trusted() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "9.9.9.9, 10.0.0.1".parse().unwrap());
        let peer: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert_eq!(client_ip(&h, Some(peer), false), "127.0.0.1");
        assert_eq!(client_ip(&h, Some(peer), true), "9.9.9.9");
        h.insert("fly-client-ip", "8.8.8.8".parse().unwrap());
        assert_eq!(client_ip(&h, Some(peer), true), "8.8.8.8");
        assert_eq!(client_ip(&HeaderMap::new(), None, true), "unknown");
    }
}
