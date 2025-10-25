/*
    SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: Apache-2.0
*/
use log::{debug, info, warn};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use pnet::packet::ip::IpNextHeaderProtocol;
use std::net::Ipv4Addr;

#[derive(Debug)]
pub struct Security {
    background_task_period: Duration, // Period for the background cleanup task
    cancel_token: Mutex<CancellationToken>, // Token to allow graceful cancellation of background tasks
    rate_limiter: Mutex<RateLimiter>,
}

/// Represents a rate limiter for (src_ip, protocol, dest_port) tuples.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    pub enabled: bool, // Flag to enable or disable rate limiting
    routes: HashMap<(Ipv4Addr, IpNextHeaderProtocol, u16), VecDeque<Instant>>, // Key: (src_ip, protocol, dest_port)
    pub max_routes: usize, // Maximum number of unique IP/protocol/port routes to track
    pub max_requests: usize, // Max requests per time window
    pub window: Duration,  // Sliding time window
    cleanup_interval: Duration, // How often to remove stale IP
}

impl Security {
    /// Creates a new `Security` instance and spawns a background cleanup task.
    pub fn new(rate_limiter: &RateLimiter) -> Arc<Self> {
        const BACKGROUND_TASK_PERIOD: Duration = Duration::from_millis(1000);
        let security = Arc::new(Self {
            background_task_period: BACKGROUND_TASK_PERIOD,
            cancel_token: Mutex::new(CancellationToken::default()),
            rate_limiter: Mutex::new(rate_limiter.clone()),
        });

        // Spawn the background cleanup task without moving `security`
        let security_clone = security.clone();
        tokio::spawn(async move { security_clone.background_task().await });
        security
    }
    /// Background task to run periodic algorithms
    async fn background_task(self: Arc<Self>) {
        let mut interval = interval(self.background_task_period);

        let mut rate_limiter_cnt = 0;
        loop {
            let cancel_token = {
                let cancel_lock = self.cancel_token.lock().await;
                cancel_lock.clone()
            };
            tokio::select! {
                      // Check the cancellation token
                      _ = cancel_token.cancelled() => {
                        // Token was cancelled, clean up and exit task
                        warn!("Cancellation token triggered, shutting down security background task");
                        break;
                    }
                _ = async {
                    interval.tick().await;
                    let mut rate_limiter_lock = self.rate_limiter.lock().await;
                    rate_limiter_cnt = (rate_limiter_cnt + 1) % (rate_limiter_lock.cleanup_interval.as_secs()/interval.period().as_secs());

                    if rate_limiter_cnt == 0 {
                        rate_limiter_lock.cleanup_old_requests();
                    }
                }=> {}
            }
        }
    }
    /// Checks if a packet is allowed based on security rules.
    ///
    /// # Arguments
    ///
    /// * `src_ip` - The source IP address of the packet.
    /// * `protocol` - The IP protocol of the packet.
    /// * `src_port` - The source port of the packet.
    /// * `dest_port` - The destination port of the packet.
    ///
    /// # Returns
    /// A `bool` indicating whether the packet is allowed based on security rules.
    pub async fn is_packet_secure(
        self: &Arc<Self>,
        src_ip: Ipv4Addr,
        protocol: IpNextHeaderProtocol,
        src_port: u16,
        dest_port: u16,
    ) -> bool {
        if dest_port == 0 || src_port == 0 {
            return false;
        }

        let mut rate_limiter_lock = self.rate_limiter.lock().await;

        if !rate_limiter_lock.enabled {
            return true;
        }

        rate_limiter_lock.is_allowed(src_ip, protocol, dest_port)
    }

    /// Enables or disables the rate limiter dynamically.
    pub async fn set_rate_limiter(self: &Arc<Self>, rate_limiter: &RateLimiter) {
        let mut rate_limiter_lock = self.rate_limiter.lock().await;
        *rate_limiter_lock = rate_limiter.clone();
    }

    /// Sets a new cancellation token for controlling the background task.
    pub async fn set_cancel_token(self: &Arc<Self>, token: CancellationToken) {
        let mut cancel_token = self.cancel_token.lock().await;
        *cancel_token = token;
    }
}

impl RateLimiter {
    /// Creates a new rate limiter with given limits.
    pub fn new(
        enabled: bool,
        max_requests: usize,
        window: Duration,
        cleanup_interval: Duration,
        max_routes: usize,
    ) -> Self {
        Self {
            enabled,
            routes: Default::default(),
            max_routes,
            max_requests: (max_requests - 1).max(1),
            window,
            cleanup_interval,
        }
    }

    /// Checks if a request from `(src_ip, protocol, dest_port)` is allowed.
    ///
    /// # Arguments
    ///
    /// * `src_ip` - The source IP address of the request.
    /// * `protocol` - The IP protocol of the request.
    /// * `dest_port` - The destination port of the request.
    ///
    /// # Returns
    /// A `bool` indicating whether the request is allowed based on rate-limiting rules.   
    fn is_allowed(
        &mut self,
        src_ip: Ipv4Addr,
        protocol: IpNextHeaderProtocol,
        dest_port: u16,
    ) -> bool {
        let now = Instant::now();
        let key = (src_ip, protocol, dest_port);

        let len = self.routes.len();
        let timestamps = match self.routes.entry(key) {
            Entry::Vacant(_) if len >= self.max_routes => return false,
            e => e.or_insert_with(|| VecDeque::with_capacity(self.max_requests)),
        };

        // Remove expired timestamps (only keep recent ones within the window)
        timestamps.retain(|&t| now.duration_since(t) <= self.window);

        // Check if within rate limit
        if timestamps.len() < self.max_requests {
            timestamps.push_back(now);
            true
        } else {
            false
        }
    }

    /// Removes stale request timestamps from the rate limiter.
    fn cleanup_old_requests(&mut self) {
        let now = Instant::now();

        self.routes.retain(|_, timestamps| {
            timestamps
                .back()
                .is_some_and(|&t| now.duration_since(t) <= self.window)
        });

        info!("Cleanup done: Active routes num: {}", self.routes.len());
        debug!("Active routes: {:?}", self.routes);
    }
}

/* Default trait impl for RateLimiter */
impl Default for RateLimiter {
    fn default() -> Self {
        RateLimiter::new(
            false,
            4,
            Duration::from_millis(1000),
            Duration::from_millis(10000),
            50,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_cleanup_partial_removal() {
        let mut rate_limiter = RateLimiter::new(
            true,
            5,
            Duration::from_millis(100), // window duration of 100ms
            Duration::from_millis(50),
            50,
        );

        let now = Instant::now();

        let src_ip1 = Ipv4Addr::new(192, 168, 1, 1);
        let protocol1 = IpNextHeaderProtocol::new(6); // TCP
        let dest_port1 = 8080;
        let key1 = (src_ip1, protocol1, dest_port1);

        let src_ip2 = Ipv4Addr::new(192, 168, 1, 2);
        let protocol2 = IpNextHeaderProtocol::new(17); // UDP
        let dest_port2 = 9090;
        let key2 = (src_ip2, protocol2, dest_port2);

        // **Key1**: All timestamps are expired → Should be removed
        rate_limiter.routes.insert(
            key1,
            VecDeque::from(vec![
                now - Duration::from_millis(200), // Expired
                now - Duration::from_millis(150), // Expired
            ]),
        );

        // **Key2**: The oldest (`front()`) timestamp is still valid → Should remain
        rate_limiter.routes.insert(
            key2,
            VecDeque::from(vec![
                now - Duration::from_millis(90), // Still valid (within 100ms window)
                now - Duration::from_millis(50), // Still valid
            ]),
        );

        // **Before cleanup check**
        assert!(rate_limiter.routes.contains_key(&key1));
        assert!(rate_limiter.routes.contains_key(&key2));

        // **Perform cleanup operation**
        rate_limiter.cleanup_old_requests();

        // **Key1 should be completely removed**
        assert!(!rate_limiter.routes.contains_key(&key1));

        // **Key2 should remain**
        assert!(rate_limiter.routes.contains_key(&key2));
    }
}
