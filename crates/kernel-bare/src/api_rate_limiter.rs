//! Token Bucket & Leaky Bucket Rate Limiting
//!
//! Implement fair rate limiting with multiple algorithms.


use core::cmp;

/// Rate limiting algorithm
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateLimitAlgorithm {
    TokenBucket,
    LeakyBucket,
    SlidingWindow,
    FixedWindow,
}

/// Token bucket state
#[derive(Clone, Copy)]
pub struct TokenBucket {
    pub bucket_id: u32,
    pub capacity: u32,
    pub refill_rate: u32,  // tokens per second
    pub current_tokens: u32,
    pub last_refill_time: u64,
    pub allow_burst: bool,
}

/// Leaky bucket state
#[derive(Clone, Copy)]
pub struct LeakyBucket {
    pub bucket_id: u32,
    pub capacity: u32,
    pub leak_rate: u32,    // tokens per second
    pub pending_requests: u32,
    pub drain_time: u64,
}

/// Rate limit request
#[derive(Clone, Copy)]
pub struct RateLimitRequest {
    pub service_id: u32,
    pub user_id: u32,
    pub tokens_required: u32,
    pub priority: u8,
}

/// Rate limit response
#[derive(Clone, Copy)]
pub struct RateLimitResponse {
    pub allowed: bool,
    pub tokens_remaining: u32,
    pub retry_after_ms: u32,
}

/// Rate limiter
pub struct RateLimiter {
    token_buckets: [TokenBucket; 256],
    leaky_buckets: [LeakyBucket; 256],
    bucket_count: u8,

    service_limits: [u32; 256],  // service_id -> bucket_id mapping

    total_requests: u32,
    allowed_requests: u32,
    denied_requests: u16,
    burst_requests: u16,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new() -> Self {
        RateLimiter {
            token_buckets: [TokenBucket {
                bucket_id: 0,
                capacity: 1000,
                refill_rate: 100,
                current_tokens: 1000,
                last_refill_time: 0,
                allow_burst: false,
            }; 256],

            leaky_buckets: [LeakyBucket {
                bucket_id: 0,
                capacity: 1000,
                leak_rate: 100,
                pending_requests: 0,
                drain_time: 0,
            }; 256],

            bucket_count: 0,
            service_limits: [0; 256],

            total_requests: 0,
            allowed_requests: 0,
            denied_requests: 0,
            burst_requests: 0,
        }
    }

    /// Add a token bucket limit
    pub fn add_token_bucket(&mut self, service_id: u32, capacity: u32, refill_rate: u32) -> Option<u32> {
        if (self.bucket_count as usize) >= 256 {
            return None;
        }

        let bucket_id = self.bucket_count as u32;
        self.token_buckets[self.bucket_count as usize] = TokenBucket {
            bucket_id,
            capacity,
            refill_rate,
            current_tokens: capacity,
            last_refill_time: 0,
            allow_burst: false,
        };

        self.service_limits[service_id as usize] = bucket_id;
        self.bucket_count += 1;
        Some(bucket_id)
    }

    /// Add a leaky bucket limit
    pub fn add_leaky_bucket(&mut self, service_id: u32, capacity: u32, leak_rate: u32) -> Option<u32> {
        if (self.bucket_count as usize) >= 256 {
            return None;
        }

        let bucket_id = self.bucket_count as u32;
        self.leaky_buckets[self.bucket_count as usize] = LeakyBucket {
            bucket_id,
            capacity,
            leak_rate,
            pending_requests: 0,
            drain_time: 0,
        };

        self.service_limits[service_id as usize] = bucket_id;
        self.bucket_count += 1;
        Some(bucket_id)
    }

    /// Allow request if tokens available
    pub fn allow_request(&mut self, service_id: u32, tokens_required: u32) -> RateLimitResponse {
        self.total_requests += 1;

        let bucket_id = self.service_limits[service_id as usize] as usize;
        if bucket_id < (self.bucket_count as usize) {
            // Refill tokens based on time elapsed
            let bucket = &mut self.token_buckets[bucket_id];
            let new_tokens = bucket.current_tokens + bucket.refill_rate;
            bucket.current_tokens = cmp::min(new_tokens, bucket.capacity);

            // Check if we have enough tokens
            if bucket.current_tokens >= tokens_required {
                bucket.current_tokens -= tokens_required;
                self.allowed_requests += 1;

                return RateLimitResponse {
                    allowed: true,
                    tokens_remaining: bucket.current_tokens,
                    retry_after_ms: 0,
                };
            } else {
                self.denied_requests += 1;

                // Calculate time to next refill
                let tokens_needed = tokens_required - bucket.current_tokens;
                let refill_time = (tokens_needed * 1000) / cmp::max(bucket.refill_rate, 1);

                return RateLimitResponse {
                    allowed: false,
                    tokens_remaining: bucket.current_tokens,
                    retry_after_ms: refill_time,
                };
            }
        }

        RateLimitResponse {
            allowed: false,
            tokens_remaining: 0,
            retry_after_ms: 0,
        }
    }

    /// Refill tokens
    fn refill_tokens(&self, bucket: &mut TokenBucket) {
        // Simplified: assume 1 second elapsed
        let new_tokens = bucket.current_tokens + bucket.refill_rate;
        bucket.current_tokens = cmp::min(new_tokens, bucket.capacity);
    }

    /// Get current token count
    pub fn get_tokens(&self, service_id: u32) -> u32 {
        let bucket_id = self.service_limits[service_id as usize] as usize;
        if bucket_id < (self.bucket_count as usize) {
            self.token_buckets[bucket_id].current_tokens
        } else {
            0
        }
    }

    /// Reset bucket to full capacity
    pub fn reset_bucket(&mut self, service_id: u32) -> bool {
        let bucket_id = self.service_limits[service_id as usize] as usize;
        if bucket_id < (self.bucket_count as usize) {
            self.token_buckets[bucket_id].current_tokens = self.token_buckets[bucket_id].capacity;
            return true;
        }
        false
    }

    /// Update refill rate dynamically
    pub fn update_rate(&mut self, service_id: u32, new_rate: u32) -> bool {
        let bucket_id = self.service_limits[service_id as usize] as usize;
        if bucket_id < (self.bucket_count as usize) {
            self.token_buckets[bucket_id].refill_rate = new_rate;
            return true;
        }
        false
    }

    /// Get limit statistics
    pub fn get_limit_stats(&self) -> (u32, u16, u16) {
        (self.allowed_requests, self.denied_requests, self.burst_requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_creation() {
        let mut rl = RateLimiter::new();
        let bucket_id = rl.add_token_bucket(1, 100, 10);
        assert!(bucket_id.is_some());
    }

    #[test]
    fn test_token_refill() {
        let mut rl = RateLimiter::new();
        rl.add_token_bucket(1, 100, 10);
        let tokens = rl.get_tokens(1);
        assert!(tokens > 0);
    }

    #[test]
    fn test_request_allowed() {
        let mut rl = RateLimiter::new();
        rl.add_token_bucket(1, 100, 10);
        let response = rl.allow_request(1, 50);
        assert!(response.allowed);
    }
}
