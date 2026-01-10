// RAYOS Phase 28 Task 3: Content Streaming & Buffering
// Streaming protocol support for audio/video with adaptive buffering
// File: crates/kernel-bare/src/content_streaming.rs
// Lines: 700+ | Tests: 13 unit + 5 scenario | Markers: 5


const MAX_SEGMENTS: usize = 64;
const MAX_PLAYLIST_ENTRIES: usize = 32;
const MAX_BUFFER_SIZE: usize = 8192;
const MAX_BANDWIDTH_HISTORY: usize = 16;
const MIN_BUFFER_THRESHOLD: usize = 512;
const MAX_BUFFER_THRESHOLD: usize = 7168;

// ============================================================================
// STREAMING FORMAT & SEGMENT DEFINITIONS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamFormat {
    HLS,       // HTTP Live Streaming
    DASH,      // Dynamic Adaptive Streaming over HTTP
    Progressive, // Progressive download
    RTP,       // Real-Time Protocol
}

impl StreamFormat {
    pub fn to_str(&self) -> &'static str {
        match self {
            StreamFormat::HLS => "HLS",
            StreamFormat::DASH => "DASH",
            StreamFormat::Progressive => "Progressive",
            StreamFormat::RTP => "RTP",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MediaSegment {
    pub segment_id: u32,
    pub duration_ms: u32,
    pub bandwidth_bps: u32,
    pub byte_offset: u32,
    pub byte_size: u32,
    pub is_keyframe: bool,
}

impl MediaSegment {
    pub fn new(segment_id: u32, duration_ms: u32, bandwidth_bps: u32) -> Self {
        MediaSegment {
            segment_id,
            duration_ms,
            bandwidth_bps,
            byte_offset: 0,
            byte_size: 0,
            is_keyframe: segment_id % 4 == 0,
        }
    }

    pub fn estimated_bytes(&self) -> u32 {
        // Simple estimation: bandwidth_bps * duration_ms / 8000
        (self.bandwidth_bps * self.duration_ms) / 8000
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlaylistEntry {
    pub entry_id: u32,
    pub bandwidth: u32,
    pub resolution: (u16, u16),
    pub is_variant: bool,
}

impl PlaylistEntry {
    pub fn new(entry_id: u32, bandwidth: u32) -> Self {
        PlaylistEntry {
            entry_id,
            bandwidth,
            resolution: (1920, 1080),
            is_variant: false,
        }
    }

    pub fn set_resolution(&mut self, width: u16, height: u16) {
        self.resolution = (width, height);
    }
}

// ============================================================================
// BUFFER & ADAPTIVE BITRATE
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BufferingState {
    Empty,
    Buffering,
    Ready,
    Playing,
    Stalled,
}

pub struct StreamBuffer {
    pub data: [u8; MAX_BUFFER_SIZE],
    pub write_pos: usize,
    pub read_pos: usize,
    pub buffer_len: usize,
    pub state: BufferingState,
    pub total_buffered: u64,
}

impl StreamBuffer {
    pub fn new() -> Self {
        StreamBuffer {
            data: [0; MAX_BUFFER_SIZE],
            write_pos: 0,
            read_pos: 0,
            buffer_len: 0,
            state: BufferingState::Empty,
            total_buffered: 0,
        }
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = MAX_BUFFER_SIZE - self.buffer_len;
        let to_write = if data.len() > available {
            available
        } else {
            data.len()
        };

        // Copy data to circular buffer
        for i in 0..to_write {
            let pos = (self.write_pos + i) % MAX_BUFFER_SIZE;
            self.data[pos] = data[i];
        }

        self.write_pos = (self.write_pos + to_write) % MAX_BUFFER_SIZE;
        self.buffer_len += to_write;
        self.total_buffered += to_write as u64;

        if self.buffer_len > 0 && self.state == BufferingState::Empty {
            self.state = BufferingState::Buffering;
        }

        to_write
    }

    pub fn read(&mut self, size: usize) -> usize {
        let to_read = if size > self.buffer_len {
            self.buffer_len
        } else {
            size
        };

        self.read_pos = (self.read_pos + to_read) % MAX_BUFFER_SIZE;
        self.buffer_len -= to_read;

        if self.buffer_len == 0 {
            self.state = BufferingState::Empty;
        }

        to_read
    }

    pub fn is_ready(&self) -> bool {
        self.buffer_len > MIN_BUFFER_THRESHOLD
    }

    pub fn utilization_percent(&self) -> u8 {
        ((self.buffer_len as u32 * 100) / MAX_BUFFER_SIZE as u32) as u8
    }
}

impl Default for StreamBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BufferingStrategy {
    Conservative,  // Prefer stability, lower bitrate
    Balanced,      // Balance quality and stability
    Aggressive,    // Prefer quality, higher bitrate
}

impl BufferingStrategy {
    pub fn target_buffer_percent(&self) -> u8 {
        match self {
            BufferingStrategy::Conservative => 40,
            BufferingStrategy::Balanced => 60,
            BufferingStrategy::Aggressive => 80,
        }
    }

    pub fn select_bitrate(&self, available_bps: u32) -> u32 {
        match self {
            BufferingStrategy::Conservative => (available_bps * 70) / 100,
            BufferingStrategy::Balanced => (available_bps * 85) / 100,
            BufferingStrategy::Aggressive => (available_bps * 100) / 100,
        }
    }
}

// ============================================================================
// BITRATE ESTIMATION
// ============================================================================

pub struct BitrateEstimator {
    pub bandwidth_history: [u32; MAX_BANDWIDTH_HISTORY],
    pub history_count: usize,
    pub current_bandwidth: u32,
    pub min_bandwidth: u32,
    pub max_bandwidth: u32,
}

impl BitrateEstimator {
    pub fn new() -> Self {
        BitrateEstimator {
            bandwidth_history: [0; MAX_BANDWIDTH_HISTORY],
            history_count: 0,
            current_bandwidth: 1000000, // 1 Mbps default
            min_bandwidth: 500000,      // 500 kbps
            max_bandwidth: 10000000,    // 10 Mbps
        }
    }

    pub fn add_measurement(&mut self, bandwidth_bps: u32) {
        let clamped = if bandwidth_bps < self.min_bandwidth {
            self.min_bandwidth
        } else if bandwidth_bps > self.max_bandwidth {
            self.max_bandwidth
        } else {
            bandwidth_bps
        };

        if self.history_count < MAX_BANDWIDTH_HISTORY {
            self.bandwidth_history[self.history_count] = clamped;
            self.history_count += 1;
        } else {
            // Shift history
            for i in 0..MAX_BANDWIDTH_HISTORY - 1 {
                self.bandwidth_history[i] = self.bandwidth_history[i + 1];
            }
            self.bandwidth_history[MAX_BANDWIDTH_HISTORY - 1] = clamped;
        }

        self.update_estimate();
    }

    fn update_estimate(&mut self) {
        if self.history_count == 0 {
            self.current_bandwidth = 1000000;
            return;
        }

        let sum: u32 = self.bandwidth_history[..self.history_count].iter().sum();
        self.current_bandwidth = sum / self.history_count as u32;
    }

    pub fn get_estimated_bandwidth(&self) -> u32 {
        self.current_bandwidth
    }

    pub fn get_stability_percent(&self) -> u8 {
        if self.history_count < 2 {
            return 100;
        }

        let avg = self.current_bandwidth;
        let mut variance = 0u32;

        for i in 0..self.history_count {
            let diff = if self.bandwidth_history[i] > avg {
                self.bandwidth_history[i] - avg
            } else {
                avg - self.bandwidth_history[i]
            };
            variance = variance.saturating_add(diff);
        }

        let avg_deviation = variance / self.history_count as u32;
        if avg_deviation == 0 {
            100
        } else {
            let stability = ((avg - avg_deviation) as u32 * 100) / avg;
            stability.min(100) as u8
        }
    }
}

impl Default for BitrateEstimator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// STREAM CLIENT & SERVER
// ============================================================================

pub struct StreamClient {
    pub client_id: u32,
    pub buffer: StreamBuffer,
    pub strategy: BufferingStrategy,
    pub current_segment: u32,
    pub segments_downloaded: u32,
    pub segments_played: u32,
    pub bitrate_estimator: BitrateEstimator,
}

impl StreamClient {
    pub fn new(client_id: u32) -> Self {
        StreamClient {
            client_id,
            buffer: StreamBuffer::new(),
            strategy: BufferingStrategy::Balanced,
            current_segment: 0,
            segments_downloaded: 0,
            segments_played: 0,
            bitrate_estimator: BitrateEstimator::new(),
        }
    }

    pub fn set_strategy(&mut self, strategy: BufferingStrategy) {
        self.strategy = strategy;
    }

    pub fn download_segment(&mut self, segment: &MediaSegment) -> bool {
        let bytes = segment.estimated_bytes();
        let written = self.buffer.write(&[0; 1024][..bytes.min(1024) as usize]);

        if written > 0 {
            self.segments_downloaded += 1;
            self.bitrate_estimator.add_measurement(segment.bandwidth_bps);
            return true;
        }
        false
    }

    pub fn play_segment(&mut self) -> bool {
        if self.buffer.buffer_len > 0 {
            self.buffer.read(512);
            self.segments_played += 1;
            self.current_segment += 1;
            return true;
        }
        false
    }

    pub fn get_buffer_health(&self) -> u8 {
        self.buffer.utilization_percent()
    }

    pub fn needs_rebuffer(&self) -> bool {
        self.buffer.buffer_len < MIN_BUFFER_THRESHOLD
    }
}

pub struct StreamServer {
    pub server_id: u32,
    pub segments: [Option<MediaSegment>; MAX_SEGMENTS],
    pub segment_count: usize,
    pub total_segments_served: u64,
    pub total_bytes_served: u64,
    pub active_clients: usize,
}

impl StreamServer {
    pub fn new(server_id: u32) -> Self {
        StreamServer {
            server_id,
            segments: [None; MAX_SEGMENTS],
            segment_count: 0,
            total_segments_served: 0,
            total_bytes_served: 0,
            active_clients: 0,
        }
    }

    pub fn add_segment(&mut self, segment: MediaSegment) -> bool {
        if self.segment_count >= MAX_SEGMENTS {
            return false;
        }
        self.segments[self.segment_count] = Some(segment);
        self.segment_count += 1;
        true
    }

    pub fn get_segment(&mut self, segment_id: u32) -> Option<MediaSegment> {
        for i in 0..self.segment_count {
            if let Some(seg) = self.segments[i] {
                if seg.segment_id == segment_id {
                    self.total_segments_served += 1;
                    self.total_bytes_served += seg.estimated_bytes() as u64;
                    return Some(seg);
                }
            }
        }
        None
    }

    pub fn register_client(&mut self) {
        self.active_clients = self.active_clients.saturating_add(1);
    }

    pub fn unregister_client(&mut self) {
        self.active_clients = self.active_clients.saturating_sub(1);
    }
}

// ============================================================================
// STREAMING METRICS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct StreamMetrics {
    pub buffer_depth_ms: u32,
    pub current_bitrate_bps: u32,
    pub latency_ms: u32,
    pub segment_duration_ms: u32,
    pub rebuffer_count: u32,
    pub total_play_duration_ms: u64,
}

impl StreamMetrics {
    pub fn new() -> Self {
        StreamMetrics {
            buffer_depth_ms: 0,
            current_bitrate_bps: 1000000,
            latency_ms: 0,
            segment_duration_ms: 0,
            rebuffer_count: 0,
            total_play_duration_ms: 0,
        }
    }

    pub fn update_buffer_depth(&mut self, buffer_bytes: usize, bitrate_bps: u32) {
        // Convert bytes to milliseconds: (bytes * 8 * 1000) / bitrate_bps
        self.buffer_depth_ms = ((buffer_bytes as u32 * 8 * 1000) / bitrate_bps).min(60000);
    }

    pub fn add_rebuffer(&mut self) {
        self.rebuffer_count = self.rebuffer_count.saturating_add(1);
    }

    pub fn quality_score(&self) -> u8 {
        // Simple quality calculation based on bitrate and rebuffers
        let bitrate_score = (self.current_bitrate_bps / 100000).min(100) as u8;
        let rebuffer_penalty = (self.rebuffer_count * 10).min(100) as u8;
        bitrate_score.saturating_sub(rebuffer_penalty)
    }
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_format_to_str() {
        assert_eq!(StreamFormat::HLS.to_str(), "HLS");
        assert_eq!(StreamFormat::DASH.to_str(), "DASH");
    }

    #[test]
    fn test_media_segment_new() {
        let seg = MediaSegment::new(1, 5000, 2000000);
        assert_eq!(seg.segment_id, 1);
        assert_eq!(seg.duration_ms, 5000);
    }

    #[test]
    fn test_media_segment_estimated_bytes() {
        let seg = MediaSegment::new(1, 5000, 2000000);
        let bytes = seg.estimated_bytes();
        assert!(bytes > 0);
    }

    #[test]
    fn test_stream_buffer_write() {
        let mut buf = StreamBuffer::new();
        let data = [1, 2, 3, 4, 5];
        let written = buf.write(&data);
        assert_eq!(written, 5);
        assert_eq!(buf.buffer_len, 5);
    }

    #[test]
    fn test_stream_buffer_read() {
        let mut buf = StreamBuffer::new();
        buf.write(&[1, 2, 3, 4, 5]);
        let read = buf.read(3);
        assert_eq!(read, 3);
        assert_eq!(buf.buffer_len, 2);
    }

    #[test]
    fn test_stream_buffer_is_ready() {
        let mut buf = StreamBuffer::new();
        assert!(!buf.is_ready());
        buf.write(&[0; 600]);
        assert!(buf.is_ready());
    }

    #[test]
    fn test_buffering_strategy_target() {
        assert_eq!(
            BufferingStrategy::Conservative.target_buffer_percent(),
            40
        );
        assert_eq!(BufferingStrategy::Balanced.target_buffer_percent(), 60);
    }

    #[test]
    fn test_bitrate_estimator_new() {
        let est = BitrateEstimator::new();
        assert_eq!(est.get_estimated_bandwidth(), 1000000);
    }

    #[test]
    fn test_bitrate_estimator_add_measurement() {
        let mut est = BitrateEstimator::new();
        est.add_measurement(2000000);
        assert!(est.get_estimated_bandwidth() > 0);
    }

    #[test]
    fn test_stream_client_new() {
        let client = StreamClient::new(1);
        assert_eq!(client.client_id, 1);
        assert_eq!(client.segments_downloaded, 0);
    }

    #[test]
    fn test_stream_client_download_segment() {
        let mut client = StreamClient::new(1);
        let seg = MediaSegment::new(1, 5000, 2000000);
        assert!(client.download_segment(&seg));
        assert_eq!(client.segments_downloaded, 1);
    }

    #[test]
    fn test_stream_server_new() {
        let server = StreamServer::new(1);
        assert_eq!(server.server_id, 1);
        assert_eq!(server.segment_count, 0);
    }

    #[test]
    fn test_stream_server_add_segment() {
        let mut server = StreamServer::new(1);
        let seg = MediaSegment::new(1, 5000, 2000000);
        assert!(server.add_segment(seg));
        assert_eq!(server.segment_count, 1);
    }

    #[test]
    fn test_stream_metrics_quality_score() {
        let mut metrics = StreamMetrics::new();
        metrics.current_bitrate_bps = 5000000;
        metrics.rebuffer_count = 0;
        let score = metrics.quality_score();
        assert!(score > 0);
    }

    #[test]
    fn test_playlist_entry_new() {
        let entry = PlaylistEntry::new(1, 2000000);
        assert_eq!(entry.entry_id, 1);
        assert_eq!(entry.bandwidth, 2000000);
    }

    #[test]
    fn test_playlist_entry_set_resolution() {
        let mut entry = PlaylistEntry::new(1, 2000000);
        entry.set_resolution(1280, 720);
        assert_eq!(entry.resolution, (1280, 720));
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_streaming_session_scenario() {
        let mut client = StreamClient::new(1);
        let mut server = StreamServer::new(1);

        server.register_client();

        let seg1 = MediaSegment::new(1, 5000, 2000000);
        let seg2 = MediaSegment::new(2, 5000, 2000000);

        server.add_segment(seg1);
        server.add_segment(seg2);

        assert!(client.download_segment(&seg1));
        assert_eq!(client.segments_downloaded, 1);

        server.unregister_client();
        assert_eq!(server.active_clients, 0);
    }

    #[test]
    fn test_adaptive_bitrate_scenario() {
        let mut client = StreamClient::new(1);
        client.set_strategy(BufferingStrategy::Aggressive);

        let seg = MediaSegment::new(1, 5000, 3000000);
        client.download_segment(&seg);

        let bandwidth = client.bitrate_estimator.get_estimated_bandwidth();
        let selected = client.strategy.select_bitrate(bandwidth);
        assert!(selected > 0);
    }

    #[test]
    fn test_buffer_underflow_scenario() {
        let mut client = StreamClient::new(1);
        assert!(!client.buffer.is_ready());

        let seg = MediaSegment::new(1, 5000, 2000000);
        client.download_segment(&seg);

        // Simulate playback
        while client.play_segment() {}

        assert!(client.needs_rebuffer());
    }

    #[test]
    fn test_multiple_segment_streaming_scenario() {
        let mut server = StreamServer::new(1);

        for i in 1..=10 {
            let seg = MediaSegment::new(i, 5000, 2000000);
            let _ = server.add_segment(seg);
        }

        assert_eq!(server.segment_count, 10);

        let retrieved = server.get_segment(5);
        assert!(retrieved.is_some());
        assert!(server.total_segments_served > 0);
    }

    #[test]
    fn test_streaming_metrics_scenario() {
        let mut metrics = StreamMetrics::new();
        metrics.update_buffer_depth(4096, 2000000);
        metrics.add_rebuffer();

        assert!(metrics.buffer_depth_ms > 0);
        assert_eq!(metrics.rebuffer_count, 1);
        let score = metrics.quality_score();
        assert!(score > 0);
    }
}
