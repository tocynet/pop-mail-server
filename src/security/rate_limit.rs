//! Rate limiting implementation

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorLimiter,
};

/// Rate limiter for protecting against abuse
pub struct RateLimiter {
    limiter: Arc<GovernorLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the specified requests per second
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::MIN));
        let limiter = Arc::new(GovernorLimiter::direct(quota));

        Self { limiter }
    }

    /// Check if a request from the given IP should be allowed
    pub fn check(&self, _ip: IpAddr) -> bool {
        // For now, use a simple global rate limiter
        // In production, you might want per-IP rate limiting
        self.limiter.check().is_ok()
    }

    /// Check if a request should be allowed (without IP)
    pub fn check_global(&self) -> bool {
        self.limiter.check().is_ok()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(2);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // First few requests should succeed
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));

        // Eventually should be rate limited
        let mut limited = false;
        for _ in 0..100 {
            if !limiter.check(ip) {
                limited = true;
                break;
            }
        }
        assert!(limited);
    }
}
