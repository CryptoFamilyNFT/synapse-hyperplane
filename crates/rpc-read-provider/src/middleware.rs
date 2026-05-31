//! RPC Middleware
//!
//! Rate limiting, request logging, and other cross-cutting concerns

use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Rate limiter using sliding window
pub struct RateLimiter {
    /// Requests per second limit
    limit: usize,
    /// Per-IP tracking
    ip_windows: RwLock<HashMap<String, IpRateLimitState>>,
}

struct IpRateLimitState {
    /// Request timestamps in current window
    timestamps: Vec<Instant>,
    /// Window start time
    window_start: Instant,
}

impl RateLimiter {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            ip_windows: RwLock::new(HashMap::new()),
        }
    }

    /// Check if request is allowed
    pub fn allow(&self, ip: &str) -> bool {
        let mut windows = self.ip_windows.write();
        
        let now = Instant::now();
        let window_duration = Duration::from_secs(1);
        
        let state = windows.entry(ip.to_string()).or_insert_with(|| IpRateLimitState {
            timestamps: Vec::with_capacity(self.limit),
            window_start: now,
        });
        
        // Reset window if expired
        if now.duration_since(state.window_start) > window_duration {
            state.timestamps.clear();
            state.window_start = now;
        }
        
        // Remove old timestamps
        state.timestamps.retain(|&ts| now.duration_since(ts) < window_duration);
        
        // Check limit
        if state.timestamps.len() >= self.limit {
            return false;
        }
        
        // Record request
        state.timestamps.push(now);
        true
    }

    /// Get current request count for IP
    pub fn get_count(&self, ip: &str) -> usize {
        let windows = self.ip_windows.read();
        
        if let Some(state) = windows.get(ip) {
            let now = Instant::now();
            let window_duration = Duration::from_secs(1);
            state.timestamps.iter().filter(|&&ts| now.duration_since(ts) < window_duration).count()
        } else {
            0
        }
    }

    /// Clear all rate limit state
    pub fn clear(&self) {
        self.ip_windows.write().clear();
    }
}

/// Request logger
pub struct RequestLogger {
    /// Enable logging
    enabled: bool,
    /// Log slow requests (> ms)
    slow_threshold_ms: u128,
}

impl RequestLogger {
    pub fn new(enabled: bool, slow_threshold_ms: u128) -> Self {
        Self {
            enabled,
            slow_threshold_ms,
        }
    }

    /// Log request start
    pub fn start(&self) -> RequestTimer {
        RequestTimer {
            start: Instant::now(),
            threshold: self.slow_threshold_ms,
            enabled: self.enabled,
        }
    }
}

/// Request timer (RAII)
pub struct RequestTimer {
    start: Instant,
    threshold: u128,
    enabled: bool,
}

impl Drop for RequestTimer {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        
        let duration = self.start.elapsed().as_millis();
        
        if duration > self.threshold {
            tracing::warn!("Slow request: {}ms", duration);
        } else {
            tracing::debug!("Request completed: {}ms", duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5);
        
        // First 5 requests should succeed
        for _ in 0..5 {
            assert!(limiter.allow("127.0.0.1"));
        }
        
        // 6th should fail
        assert!(!limiter.allow("127.0.0.1"));
        
        // Different IP should succeed
        assert!(limiter.allow("192.168.1.1"));
    }

    #[test]
    fn test_rate_limiter_window_reset() {
        let limiter = RateLimiter::new(2);
        
        // Use up limit
        assert!(limiter.allow("127.0.0.1"));
        assert!(limiter.allow("127.0.0.1"));
        assert!(!limiter.allow("127.0.0.1"));
        
        // Wait for window to reset
        std::thread::sleep(Duration::from_secs(1) + Duration::from_millis(100));
        
        // Should succeed again
        assert!(limiter.allow("127.0.0.1"));
    }
}
