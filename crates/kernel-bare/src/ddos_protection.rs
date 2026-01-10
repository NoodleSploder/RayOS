//! DDoS Protection & Rate Limiting
//!
//! Rate limiting, SYN flood mitigation, anomaly detection, and traffic policing.


use core::cmp;

/// Attack type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackType {
    SynFlood,
    UdpFlood,
    IcmpFlood,
    DnsAmplification,
    Slowloris,
    Volumetric,
    None,
}

/// Flow metric
#[derive(Clone, Copy)]
pub struct FlowMetric {
    pub flow_id: u32,
    pub packets: u32,
    pub bytes: u64,
    pub timestamp: u64,
    pub rate_bps: u32,
}

/// Traffic policy
#[derive(Clone, Copy)]
pub struct TrafficPolicy {
    pub max_rate_bps: u32,
    pub burst_size: u32,
    pub max_packet_rate: u32,
    pub timeout: u32,
}

/// Rate limiter for individual flow
pub struct RateLimiter {
    flow_id: u32,
    tokens: u32,
    max_tokens: u32,
    refill_rate: u32,
    last_refill: u64,
    policy: TrafficPolicy,
}

impl RateLimiter {
    /// Create default instance
    fn default() -> Self {
        RateLimiter {
            flow_id: 0,
            tokens: 1000,
            max_tokens: 1000,
            refill_rate: 100,
            last_refill: 0,
            policy: TrafficPolicy {
                max_rate_bps: 0,
                burst_size: 0,
                max_packet_rate: 0,
                timeout: 0,
            },
        }
    }
}

/// Flow tracking
#[derive(Clone, Copy)]
pub struct TrackedFlow {
    flow_id: u32,
    source_ip: u32,
    dest_ip: u32,
    protocol: u8,
    packets: u32,
    bytes: u64,
    syn_count: u16,
    fin_count: u16,
    last_seen: u64,
}

/// DDoS detector
pub struct DDoSProtection {
    flows: [TrackedFlow; 512],
    flow_count: u16,

    limiters: [Option<RateLimiter>; 128],
    limiter_count: u8,

    policies: [TrafficPolicy; 16],
    policy_count: u8,

    attack_type: AttackType,
    attack_score: u16,

    syn_flood_threshold: u32,
    udp_flood_threshold: u32,

    packets_dropped: u32,
    flows_throttled: u32,
    attacks_detected: u16,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(flow_id: u32, policy: TrafficPolicy) -> Self {
        RateLimiter {
            flow_id,
            tokens: policy.burst_size,
            max_tokens: policy.burst_size,
            refill_rate: policy.max_rate_bps / 8, // Convert to bytes/sec
            last_refill: 0,
            policy,
        }
    }

    /// Check if packet can be sent
    pub fn allow_packet(&mut self, packet_size: u32, current_time: u64) -> bool {
        // Refill tokens based on elapsed time
        let elapsed = current_time - self.last_refill;
        let tokens_to_add = ((self.refill_rate as u64 * elapsed) / 1000) as u32;
        self.tokens = cmp::min(self.tokens + tokens_to_add, self.max_tokens);
        self.last_refill = current_time;

        // Check if we have enough tokens
        if self.tokens >= packet_size {
            self.tokens -= packet_size;
            true
        } else {
            false
        }
    }

    /// Get current token count
    pub fn get_tokens(&self) -> u32 {
        self.tokens
    }
}

impl DDoSProtection {
    /// Create new DDoS protection
    pub fn new() -> Self {
        // Initialize limiters array - can't use [None; 128] because RateLimiter doesn't impl Copy
        let limiters: [Option<RateLimiter>; 128] = [
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
        ];

        DDoSProtection {
            flows: [TrackedFlow {
                flow_id: 0,
                source_ip: 0,
                dest_ip: 0,
                protocol: 0,
                packets: 0,
                bytes: 0,
                syn_count: 0,
                fin_count: 0,
                last_seen: 0,
            }; 512],
            flow_count: 0,

            limiters,
            limiter_count: 0,

            policies: [TrafficPolicy {
                max_rate_bps: 1_000_000, // 1 Mbps default
                burst_size: 1000,
                max_packet_rate: 10000,
                timeout: 300,
            }; 16],
            policy_count: 1,

            attack_type: AttackType::None,
            attack_score: 0,

            syn_flood_threshold: 100,
            udp_flood_threshold: 1000,

            packets_dropped: 0,
            flows_throttled: 0,
            attacks_detected: 0,
        }
    }

    /// Check rate limit for flow
    pub fn check_rate_limit(&mut self, flow_id: u32, packet_size: u32, current_time: u64) -> bool {
        // Find or create limiter
        for i in 0..(self.limiter_count as usize) {
            if let Some(limiter) = &mut self.limiters[i] {
                if limiter.flow_id == flow_id {
                    return limiter.allow_packet(packet_size, current_time);
                }
            }
        }

        // Create new limiter
        if (self.limiter_count as usize) < 128 {
            let policy = self.policies[0];
            self.limiters[self.limiter_count as usize] = Some(RateLimiter::new(flow_id, policy));
            self.limiter_count += 1;
            if let Some(limiter) = &mut self.limiters[(self.limiter_count - 1) as usize] {
                return limiter.allow_packet(packet_size, current_time);
            }
        }

        false
    }

    /// Detect SYN flood attack
    pub fn detect_syn_flood(&mut self) -> bool {
        let mut syn_count = 0u32;
        for i in 0..(self.flow_count as usize) {
            syn_count += self.flows[i].syn_count as u32;
        }

        if syn_count > self.syn_flood_threshold {
            self.attack_type = AttackType::SynFlood;
            self.attack_score = ((syn_count / 10) as u16).min(1000);
            self.attacks_detected += 1;
            return true;
        }

        false
    }

    /// Validate packet source
    pub fn validate_source(&self, source_ip: u32, current_time: u64) -> bool {
        // Simple validation: check if source is in tracked flows
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].source_ip == source_ip {
                // Check if flow is timing out
                if current_time > (self.flows[i].last_seen + 3600) {
                    return false; // Timed out
                }
                return true;
            }
        }

        // Unknown source is allowed initially
        true
    }

    /// Apply traffic policy
    pub fn apply_policy(&mut self, flow_id: u32, policy_id: usize) -> bool {
        if policy_id >= (self.policy_count as usize) {
            return false;
        }

        let policy = self.policies[policy_id];

        for i in 0..(self.limiter_count as usize) {
            if let Some(limiter) = &mut self.limiters[i] {
                if limiter.flow_id == flow_id {
                    limiter.policy = policy;
                    return true;
                }
            }
        }

        false
    }

    /// Calculate anomaly score
    pub fn calculate_anomaly(&mut self, packets: u32, bytes: u64) -> u16 {
        let mut score = 0u16;

        // Rate-based anomaly
        if packets > 10000 {
            score += 100;
        }

        // Size-based anomaly
        if bytes > 100_000_000 {
            score += 100;
        }

        // Connection-based anomaly
        if (self.flow_count as usize) > 400 {
            score += 50;
        }

        self.attack_score = (self.attack_score + score).min(1000);
        score
    }

    /// Throttle flow
    pub fn throttle_flow(&mut self, flow_id: u32) -> bool {
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                self.flows_throttled += 1;
                return true;
            }
        }
        false
    }

    /// Get DDoS status
    pub fn get_ddos_status(&self) -> (AttackType, u16) {
        (self.attack_type, self.attack_score)
    }

    /// Get packets dropped count
    pub fn get_packets_dropped(&self) -> u32 {
        self.packets_dropped
    }

    /// Get attacks detected count
    pub fn get_attacks_detected(&self) -> u16 {
        self.attacks_detected
    }

    /// Get flows throttled count
    pub fn get_flows_throttled(&self) -> u32 {
        self.flows_throttled
    }

    /// Record packet for flow tracking
    pub fn record_packet(&mut self, source_ip: u32, dest_ip: u32, protocol: u8,
                        packet_size: u32, is_syn: bool, current_time: u64) -> u32 {
        let flow_id = ((source_ip ^ dest_ip) as u32) ^ ((protocol as u32) << 8);

        // Find or create flow
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                self.flows[i].packets += 1;
                self.flows[i].bytes += packet_size as u64;
                if is_syn {
                    self.flows[i].syn_count += 1;
                }
                self.flows[i].last_seen = current_time;
                return flow_id;
            }
        }

        // Create new flow
        if (self.flow_count as usize) < 512 {
            self.flows[self.flow_count as usize] = TrackedFlow {
                flow_id,
                source_ip,
                dest_ip,
                protocol,
                packets: 1,
                bytes: packet_size as u64,
                syn_count: if is_syn { 1 } else { 0 },
                fin_count: 0,
                last_seen: current_time,
            };
            self.flow_count += 1;
        }

        flow_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let policy = TrafficPolicy {
            max_rate_bps: 1_000_000,
            burst_size: 1000,
            max_packet_rate: 10000,
            timeout: 300,
        };
        let mut limiter = RateLimiter::new(1, policy);
        assert!(limiter.allow_packet(500, 0));
    }

    #[test]
    fn test_ddos_protection() {
        let ddos = DDoSProtection::new();
        assert_eq!(ddos.get_packets_dropped(), 0);
        assert_eq!(ddos.get_attacks_detected(), 0);
    }

    #[test]
    fn test_source_validation() {
        let ddos = DDoSProtection::new();
        assert!(ddos.validate_source(0x7F000001, 0));
    }
}
