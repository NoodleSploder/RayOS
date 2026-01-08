//! Network Monitoring & Telemetry
//!
//! Real-time network statistics, flow monitoring, latency tracking, and metrics collection.

#![no_std]

use core::cmp;

/// Network interface statistics
#[derive(Clone, Copy)]
pub struct InterfaceStats {
    pub if_id: u8,
    pub packets_in: u32,
    pub packets_out: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub errors_in: u16,
    pub errors_out: u16,
    pub dropped_in: u16,
    pub dropped_out: u16,
}

/// Per-flow statistics
#[derive(Clone, Copy)]
pub struct FlowStats {
    pub flow_id: u32,
    pub source_ip: u32,
    pub dest_ip: u32,
    pub protocol: u8,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub rtt_min_us: u32,
    pub rtt_max_us: u32,
    pub rtt_avg_us: u32,
    pub packet_loss: u16, // percentage * 10
    pub state: u8, // 0=closed, 1=established, 2=closing
    pub last_activity: u64,
}

/// Latency sample
#[derive(Clone, Copy)]
pub struct LatencySample {
    pub flow_id: u32,
    pub rtt_us: u32,
    pub timestamp: u64,
}

/// Jitter tracking
#[derive(Clone, Copy)]
pub struct JitterInfo {
    pub flow_id: u32,
    pub samples: [u32; 16],
    pub sample_count: u8,
    pub jitter_us: u32,
}

/// Packet loss tracker
#[derive(Clone, Copy)]
pub struct LossTracker {
    pub flow_id: u32,
    pub sent_packets: u32,
    pub acked_packets: u32,
    pub lost_packets: u32,
    pub loss_rate: u16,
}

/// Network telemetry
pub struct NetworkTelemetry {
    interfaces: [InterfaceStats; 8],
    interface_count: u8,
    
    flows: [FlowStats; 256],
    flow_count: u16,
    
    latency_samples: [LatencySample; 1024],
    sample_count: u16,
    
    jitter_trackers: [JitterInfo; 64],
    jitter_count: u8,
    
    loss_trackers: [LossTracker; 64],
    loss_count: u8,
    
    total_packets: u64,
    total_bytes: u64,
    encryption_overhead: u64,
    total_rtt_sum: u64,
    total_samples: u32,
}

impl NetworkTelemetry {
    /// Create new network telemetry
    pub fn new() -> Self {
        NetworkTelemetry {
            interfaces: [InterfaceStats {
                if_id: 0,
                packets_in: 0,
                packets_out: 0,
                bytes_in: 0,
                bytes_out: 0,
                errors_in: 0,
                errors_out: 0,
                dropped_in: 0,
                dropped_out: 0,
            }; 8],
            interface_count: 0,
            
            flows: [FlowStats {
                flow_id: 0,
                source_ip: 0,
                dest_ip: 0,
                protocol: 0,
                packets_sent: 0,
                packets_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
                rtt_min_us: u32::MAX,
                rtt_max_us: 0,
                rtt_avg_us: 0,
                packet_loss: 0,
                state: 0,
                last_activity: 0,
            }; 256],
            flow_count: 0,
            
            latency_samples: [LatencySample {
                flow_id: 0,
                rtt_us: 0,
                timestamp: 0,
            }; 1024],
            sample_count: 0,
            
            jitter_trackers: [JitterInfo {
                flow_id: 0,
                samples: [0; 16],
                sample_count: 0,
                jitter_us: 0,
            }; 64],
            jitter_count: 0,
            
            loss_trackers: [LossTracker {
                flow_id: 0,
                sent_packets: 0,
                acked_packets: 0,
                lost_packets: 0,
                loss_rate: 0,
            }; 64],
            loss_count: 0,
            
            total_packets: 0,
            total_bytes: 0,
            encryption_overhead: 0,
            total_rtt_sum: 0,
            total_samples: 0,
        }
    }
    
    /// Record outgoing packet
    pub fn record_outgoing(&mut self, flow_id: u32, packet_size: u32, if_id: u8) {
        self.total_packets += 1;
        self.total_bytes += packet_size as u64;
        
        // Update interface stats
        if (if_id as usize) < 8 {
            self.interfaces[if_id as usize].packets_out += 1;
            self.interfaces[if_id as usize].bytes_out += packet_size as u64;
        }
        
        // Update flow stats
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                self.flows[i].packets_sent += 1;
                self.flows[i].bytes_sent += packet_size as u64;
                return;
            }
        }
    }
    
    /// Record incoming packet
    pub fn record_incoming(&mut self, flow_id: u32, packet_size: u32, if_id: u8) {
        // Update interface stats
        if (if_id as usize) < 8 {
            self.interfaces[if_id as usize].packets_in += 1;
            self.interfaces[if_id as usize].bytes_in += packet_size as u64;
        }
        
        // Update flow stats
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                self.flows[i].packets_received += 1;
                self.flows[i].bytes_received += packet_size as u64;
                return;
            }
        }
        
        // Create new flow if needed
        if (self.flow_count as usize) < 256 {
            self.flows[self.flow_count as usize] = FlowStats {
                flow_id,
                source_ip: 0,
                dest_ip: 0,
                protocol: 0,
                packets_sent: 0,
                packets_received: 1,
                bytes_sent: 0,
                bytes_received: packet_size as u64,
                rtt_min_us: u32::MAX,
                rtt_max_us: 0,
                rtt_avg_us: 0,
                packet_loss: 0,
                state: 1,
                last_activity: 0,
            };
            self.flow_count += 1;
        }
    }
    
    /// Record RTT (round trip time)
    pub fn record_rtt(&mut self, flow_id: u32, rtt_us: u32, timestamp: u64) {
        // Update flow stats
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                self.flows[i].rtt_min_us = cmp::min(self.flows[i].rtt_min_us, rtt_us);
                self.flows[i].rtt_max_us = cmp::max(self.flows[i].rtt_max_us, rtt_us);
                self.flows[i].rtt_avg_us = 
                    ((self.flows[i].rtt_avg_us as u64 * self.total_samples as u64 + rtt_us as u64) 
                    / ((self.total_samples + 1) as u64)) as u32;
                break;
            }
        }
        
        // Record sample
        if (self.sample_count as usize) < 1024 {
            self.latency_samples[self.sample_count as usize] = LatencySample {
                flow_id,
                rtt_us,
                timestamp,
            };
            self.sample_count += 1;
        }
        
        self.total_rtt_sum += rtt_us as u64;
        self.total_samples += 1;
    }
    
    /// Calculate jitter
    pub fn calculate_jitter(&mut self, flow_id: u32, rtt_us: u32) -> u32 {
        // Find or create jitter tracker
        let mut tracker_idx = None;
        for i in 0..(self.jitter_count as usize) {
            if self.jitter_trackers[i].flow_id == flow_id {
                tracker_idx = Some(i);
                break;
            }
        }
        
        if tracker_idx.is_none() && (self.jitter_count as usize) < 64 {
            self.jitter_trackers[self.jitter_count as usize].flow_id = flow_id;
            tracker_idx = Some(self.jitter_count as usize);
            self.jitter_count += 1;
        }
        
        if let Some(idx) = tracker_idx {
            let tracker = &mut self.jitter_trackers[idx];
            
            if tracker.sample_count < 16 {
                tracker.samples[tracker.sample_count as usize] = rtt_us;
                tracker.sample_count += 1;
            } else {
                // Calculate jitter from samples
                let mut sum = 0u64;
                let avg = tracker.samples.iter().sum::<u32>() / 16;
                
                for i in 0..16 {
                    let diff = if tracker.samples[i] > avg {
                        tracker.samples[i] - avg
                    } else {
                        avg - tracker.samples[i]
                    };
                    sum += diff as u64;
                }
                
                tracker.jitter_us = (sum / 16) as u32;
                
                // Shift samples
                for i in 0..15 {
                    tracker.samples[i] = tracker.samples[i + 1];
                }
                tracker.samples[15] = rtt_us;
            }
            
            self.jitter_trackers[idx].jitter_us
        } else {
            0
        }
    }
    
    /// Record packet loss
    pub fn record_packet_loss(&mut self, flow_id: u32, sent: u32, acked: u32) -> u16 {
        let lost = sent.saturating_sub(acked);
        let loss_rate = if sent > 0 {
            ((lost as u64 * 1000) / sent as u64) as u16
        } else {
            0
        };
        
        // Update loss tracker
        for i in 0..(self.loss_count as usize) {
            if self.loss_trackers[i].flow_id == flow_id {
                self.loss_trackers[i].sent_packets = sent;
                self.loss_trackers[i].acked_packets = acked;
                self.loss_trackers[i].lost_packets = lost;
                self.loss_trackers[i].loss_rate = loss_rate;
                
                // Update flow stats
                for j in 0..(self.flow_count as usize) {
                    if self.flows[j].flow_id == flow_id {
                        self.flows[j].packet_loss = loss_rate / 10; // Convert to percentage
                    }
                }
                
                return loss_rate;
            }
        }
        
        // Create new loss tracker
        if (self.loss_count as usize) < 64 {
            self.loss_trackers[self.loss_count as usize] = LossTracker {
                flow_id,
                sent_packets: sent,
                acked_packets: acked,
                lost_packets: lost,
                loss_rate,
            };
            self.loss_count += 1;
        }
        
        loss_rate
    }
    
    /// Record encryption overhead
    pub fn record_encryption_overhead(&mut self, plaintext_size: u32, ciphertext_size: u32) {
        let overhead = if ciphertext_size > plaintext_size {
            (ciphertext_size - plaintext_size) as u64
        } else {
            0
        };
        self.encryption_overhead += overhead;
    }
    
    /// Get flow statistics
    pub fn get_flow_stats(&self, flow_id: u32) -> Option<FlowStats> {
        for i in 0..(self.flow_count as usize) {
            if self.flows[i].flow_id == flow_id {
                return Some(self.flows[i]);
            }
        }
        None
    }
    
    /// Get average RTT
    pub fn get_average_rtt(&self) -> u32 {
        if self.total_samples > 0 {
            (self.total_rtt_sum / self.total_samples as u64) as u32
        } else {
            0
        }
    }
    
    /// Get interface statistics
    pub fn get_interface_stats(&self, if_id: u8) -> Option<InterfaceStats> {
        if (if_id as usize) < (self.interface_count as usize) {
            Some(self.interfaces[if_id as usize])
        } else {
            None
        }
    }
    
    /// Get total packets processed
    pub fn get_total_packets(&self) -> u64 {
        self.total_packets
    }
    
    /// Get total bytes processed
    pub fn get_total_bytes(&self) -> u64 {
        self.total_bytes
    }
    
    /// Get encryption overhead
    pub fn get_encryption_overhead(&self) -> u64 {
        self.encryption_overhead
    }
    
    /// Get flow count
    pub fn get_flow_count(&self) -> u16 {
        self.flow_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_telemetry_creation() {
        let telemetry = NetworkTelemetry::new();
        assert_eq!(telemetry.get_total_packets(), 0);
        assert_eq!(telemetry.get_total_bytes(), 0);
    }
    
    #[test]
    fn test_flow_tracking() {
        let mut telemetry = NetworkTelemetry::new();
        telemetry.record_outgoing(1, 100, 0);
        telemetry.record_incoming(1, 50, 0);
        assert_eq!(telemetry.get_total_packets(), 2);
        assert_eq!(telemetry.get_flow_count(), 1);
    }
    
    #[test]
    fn test_latency_tracking() {
        let mut telemetry = NetworkTelemetry::new();
        telemetry.record_rtt(1, 10000, 0);
        telemetry.record_rtt(1, 12000, 1);
        assert!(telemetry.get_average_rtt() > 0);
    }
}
