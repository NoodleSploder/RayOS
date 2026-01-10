//! Web Dashboard Backend: Remote Monitoring and Control Interface
//!
//! Provides HTTP/WebSocket APIs for real-time evolution metrics, control commands,
//! and data export. Enables remote monitoring and interaction with the Ouroboros Engine
//! during evolution sessions without requiring local access.
//!
//! Phase 34, Task 6

/// HTTP request method
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HttpMethod {
    Get = 0,
    Post = 1,
    Put = 2,
    Delete = 3,
}

/// HTTP status code
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum HttpStatus {
    Ok = 200,
    Created = 201,
    BadRequest = 400,
    NotFound = 404,
    InternalServerError = 500,
}

impl HttpStatus {
    /// Get status code as u16
    pub const fn code(&self) -> u16 {
        *self as u16
    }

    /// Is success status
    pub const fn is_success(&self) -> bool {
        matches!(self, HttpStatus::Ok | HttpStatus::Created)
    }
}

/// API endpoint type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EndpointType {
    MetricsGet = 0,           // GET /metrics
    HealthCheck = 1,          // GET /health
    ControlPause = 2,         // POST /control/pause
    ControlResume = 3,        // POST /control/resume
    ControlStop = 4,          // POST /control/stop
    ExportData = 5,           // GET /export
    DashboardState = 6,       // GET /dashboard/state
    HistoryQuery = 7,         // GET /history
    FrontierQuery = 8,        // GET /frontier
}

/// Export format for data export
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ApiExportFormat {
    Json = 0,      // JSON format
    Csv = 1,       // CSV format
    Binary = 2,    // Compact binary format
}

/// WebSocket message type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WebSocketMessageType {
    MetricUpdate = 0,      // Real-time metric update
    EventNotification = 1, // Evolution event
    ControlResponse = 2,   // Response to control command
    Error = 3,             // Error message
}

/// HTTP request representation
#[derive(Clone, Copy, Debug)]
pub struct HttpRequest {
    /// Request ID
    pub id: u32,
    /// HTTP method
    pub method: HttpMethod,
    /// Endpoint type
    pub endpoint: EndpointType,
    /// Query parameter (filter/option)
    pub query_param: u8,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
}

impl HttpRequest {
    /// Create new HTTP request
    pub const fn new(id: u32, method: HttpMethod, endpoint: EndpointType) -> Self {
        HttpRequest {
            id,
            method,
            endpoint,
            query_param: 0,
            timestamp_ms: 0,
        }
    }

    /// Set query parameter
    pub fn with_query(mut self, param: u8) -> Self {
        self.query_param = param;
        self
    }
}

/// HTTP response representation
#[derive(Clone, Copy, Debug)]
pub struct HttpResponse {
    /// Request ID being responded to
    pub request_id: u32,
    /// Response status
    pub status: HttpStatus,
    /// Response size (bytes)
    pub size_bytes: u16,
    /// Response time (ms)
    pub response_time_ms: u16,
}

impl HttpResponse {
    /// Create new HTTP response
    pub const fn new(request_id: u32, status: HttpStatus) -> Self {
        HttpResponse {
            request_id,
            status,
            size_bytes: 0,
            response_time_ms: 0,
        }
    }

    /// Set response details
    pub fn with_size(mut self, size: u16) -> Self {
        self.size_bytes = size;
        self
    }

    /// Set response time
    pub fn with_time(mut self, time_ms: u16) -> Self {
        self.response_time_ms = time_ms;
        self
    }
}

/// WebSocket message for real-time updates
#[derive(Clone, Copy, Debug)]
pub struct WebSocketMessage {
    /// Message ID
    pub id: u32,
    /// Message type
    pub msg_type: WebSocketMessageType,
    /// Payload size (bytes)
    pub payload_size: u16,
    /// Data content hash
    pub content_hash: u32,
}

impl WebSocketMessage {
    /// Create new WebSocket message
    pub const fn new(id: u32, msg_type: WebSocketMessageType) -> Self {
        WebSocketMessage {
            id,
            msg_type,
            payload_size: 0,
            content_hash: 0,
        }
    }

    /// Set payload size
    pub fn with_size(mut self, size: u16) -> Self {
        self.payload_size = size;
        self
    }
}

/// Dashboard metric snapshot
#[derive(Clone, Copy, Debug)]
pub struct MetricSnapshot {
    /// Metric ID
    pub id: u32,
    /// Total mutations evaluated
    pub total_mutations: u32,
    /// Successful mutations
    pub successful_mutations: u32,
    /// Current performance gain (percent)
    pub performance_gain_percent: u8,
    /// System reliability score (0-100)
    pub reliability_score: u8,
    /// Power efficiency score (0-100)
    pub power_efficiency_score: u8,
    /// Security score (0-100)
    pub security_score: u8,
    /// Uptime (seconds)
    pub uptime_seconds: u32,
}

impl MetricSnapshot {
    /// Create new metric snapshot
    pub const fn new(id: u32, total: u32, successful: u32) -> Self {
        MetricSnapshot {
            id,
            total_mutations: total,
            successful_mutations: successful,
            performance_gain_percent: 0,
            reliability_score: 0,
            power_efficiency_score: 0,
            security_score: 0,
            uptime_seconds: 0,
        }
    }

    /// Get success rate (0-100)
    pub fn success_rate(&self) -> u8 {
        if self.total_mutations == 0 {
            0
        } else {
            ((self.successful_mutations as u32 * 100) / self.total_mutations as u32) as u8
        }
    }

    /// Get average score across metrics
    pub fn average_score(&self) -> u8 {
        ((self.reliability_score as u16 + self.power_efficiency_score as u16
            + self.security_score as u16) / 3) as u8
    }
}

/// Dashboard state snapshot
#[derive(Clone, Copy, Debug)]
pub struct DashboardState {
    /// State ID
    pub id: u32,
    /// Is evolution active
    pub evolution_active: bool,
    /// Is paused
    pub paused: bool,
    /// Current phase
    pub current_phase: u8,
    /// Metrics snapshot
    pub metrics: MetricSnapshot,
    /// Connected WebSocket clients
    pub connected_clients: u16,
}

impl DashboardState {
    /// Create new dashboard state
    pub const fn new(id: u32) -> Self {
        DashboardState {
            id,
            evolution_active: true,
            paused: false,
            current_phase: 0,
            metrics: MetricSnapshot::new(id, 0, 0),
            connected_clients: 0,
        }
    }
}

/// Control command type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ControlCommand {
    Pause = 0,
    Resume = 1,
    Stop = 2,
    ConfigChange = 3,
    RecalculateFrontier = 4,
}

/// Control command request
#[derive(Clone, Copy, Debug)]
pub struct ControlRequest {
    /// Command ID
    pub id: u32,
    /// Command type
    pub command: ControlCommand,
    /// Command parameter (optional)
    pub parameter: u32,
}

impl ControlRequest {
    /// Create new control request
    pub const fn new(id: u32, command: ControlCommand) -> Self {
        ControlRequest {
            id,
            command,
            parameter: 0,
        }
    }

    /// Set parameter
    pub fn with_parameter(mut self, param: u32) -> Self {
        self.parameter = param;
        self
    }
}

/// Control response
#[derive(Clone, Copy, Debug)]
pub struct ControlResponse {
    /// Command ID being responded to
    pub command_id: u32,
    /// Success status
    pub success: bool,
    /// Status code/error code
    pub status_code: u8,
}

impl ControlResponse {
    /// Create new control response
    pub const fn new(command_id: u32, success: bool) -> Self {
        ControlResponse {
            command_id,
            success,
            status_code: 0,
        }
    }
}

/// Data export request
#[derive(Clone, Copy, Debug)]
pub struct ApiExportRequest {
    /// Export ID
    pub id: u32,
    /// Export format
    pub format: ApiExportFormat,
    /// Include metrics
    pub include_metrics: bool,
    /// Include history
    pub include_history: bool,
    /// Include frontier
    pub include_frontier: bool,
    /// Max entries (0 = all)
    pub max_entries: u32,
}

impl ApiExportRequest {
    /// Create new export request
    pub const fn new(id: u32, format: ApiExportFormat) -> Self {
        ApiExportRequest {
            id,
            format,
            include_metrics: true,
            include_history: false,
            include_frontier: false,
            max_entries: 0,
        }
    }
}

/// Export result
#[derive(Clone, Copy, Debug)]
pub struct ApiExportResult {
    /// Export ID
    pub id: u32,
    /// Success status
    pub success: bool,
    /// Exported data size (bytes)
    pub data_size_bytes: u32,
    /// Entry count
    pub entry_count: u32,
    /// Compression ratio (percent)
    pub compression_ratio: u8,
}

impl ApiExportResult {
    /// Create new export result
    pub const fn new(id: u32, success: bool) -> Self {
        ApiExportResult {
            id,
            success,
            data_size_bytes: 0,
            entry_count: 0,
            compression_ratio: 0,
        }
    }
}

/// Dashboard backend server
pub struct DashboardBackend {
    /// Current dashboard state
    state: DashboardState,
    /// HTTP request history (max 100)
    request_history: [Option<HttpRequest>; 100],
    /// HTTP response history (max 100)
    response_history: [Option<HttpResponse>; 100],
    /// WebSocket clients (max 50)
    websocket_clients: [Option<u32>; 50],
    /// Control requests pending (max 20)
    pending_commands: [Option<ControlRequest>; 20],
    /// Total HTTP requests processed
    total_requests: u32,
    /// Total WebSocket messages sent
    total_ws_messages: u32,
}

impl DashboardBackend {
    /// Create new dashboard backend
    pub const fn new() -> Self {
        DashboardBackend {
            state: DashboardState::new(0),
            request_history: [None; 100],
            response_history: [None; 100],
            websocket_clients: [None; 50],
            pending_commands: [None; 20],
            total_requests: 0,
            total_ws_messages: 0,
        }
    }

    /// Update dashboard metrics
    pub fn update_metrics(&mut self, snapshot: MetricSnapshot) {
        self.state.metrics = snapshot;
    }

    /// Handle HTTP request
    pub fn handle_request(&mut self, request: HttpRequest) -> HttpResponse {
        // Store request
        for slot in &mut self.request_history {
            if slot.is_none() {
                *slot = Some(request);
                break;
            }
        }

        self.total_requests += 1;

        // Generate response
        let status = match request.endpoint {
            EndpointType::MetricsGet | EndpointType::DashboardState | EndpointType::HistoryQuery
            | EndpointType::FrontierQuery => {
                if request.method == HttpMethod::Get {
                    HttpStatus::Ok
                } else {
                    HttpStatus::BadRequest
                }
            }
            EndpointType::ControlPause | EndpointType::ControlResume | EndpointType::ControlStop => {
                if request.method == HttpMethod::Post {
                    HttpStatus::Ok
                } else {
                    HttpStatus::BadRequest
                }
            }
            EndpointType::ExportData => {
                if request.method == HttpMethod::Get {
                    HttpStatus::Ok
                } else {
                    HttpStatus::BadRequest
                }
            }
            EndpointType::HealthCheck => HttpStatus::Ok,
        };

        let mut response = HttpResponse::new(request.id, status);

        // Store response
        for slot in &mut self.response_history {
            if slot.is_none() {
                *slot = Some(response);
                break;
            }
        }

        response
    }

    /// Register WebSocket client
    pub fn register_client(&mut self, client_id: u32) -> bool {
        for slot in &mut self.websocket_clients {
            if slot.is_none() {
                *slot = Some(client_id);
                self.state.connected_clients += 1;
                return true;
            }
        }
        false
    }

    /// Unregister WebSocket client
    pub fn unregister_client(&mut self, client_id: u32) -> bool {
        for slot in &mut self.websocket_clients {
            match slot {
                Some(id) if *id == client_id => {
                    *slot = None;
                    self.state.connected_clients = self.state.connected_clients.saturating_sub(1);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Broadcast WebSocket message to all clients
    pub fn broadcast_message(&mut self, message: WebSocketMessage) -> u16 {
        let mut count = 0;
        for slot in &self.websocket_clients {
            if slot.is_some() {
                count += 1;
            }
        }
        self.total_ws_messages += 1;
        count
    }

    /// Queue control command
    pub fn queue_command(&mut self, command: ControlRequest) -> bool {
        for slot in &mut self.pending_commands {
            if slot.is_none() {
                *slot = Some(command);
                return true;
            }
        }
        false
    }

    /// Get next pending command
    pub fn get_next_command(&mut self) -> Option<ControlRequest> {
        for slot in &mut self.pending_commands {
            if let Some(command) = slot {
                let cmd = *command;
                *slot = None;
                return Some(cmd);
            }
        }
        None
    }

    /// Execute control command
    pub fn execute_command(&mut self, command: ControlCommand) -> ControlResponse {
        let response_id = self.total_requests;
        match command {
            ControlCommand::Pause => {
                self.state.paused = true;
                ControlResponse::new(response_id as u32, true)
            }
            ControlCommand::Resume => {
                self.state.paused = false;
                ControlResponse::new(response_id as u32, true)
            }
            ControlCommand::Stop => {
                self.state.evolution_active = false;
                ControlResponse::new(response_id as u32, true)
            }
            ControlCommand::ConfigChange => {
                // Acknowledge config change
                ControlResponse::new(response_id as u32, true)
            }
            ControlCommand::RecalculateFrontier => {
                // Acknowledge recalculation request
                ControlResponse::new(response_id as u32, true)
            }
        }
    }

    /// Handle data export request
    pub fn handle_export(&self, request: ApiExportRequest) -> ApiExportResult {
        let mut result = ApiExportResult::new(request.id, true);

        // Calculate estimated data size
        let mut size = 0u32;
        if request.include_metrics {
            size += 128;  // Metrics JSON/CSV
        }
        if request.include_history {
            size += 512;  // History entries
        }
        if request.include_frontier {
            size += 256;  // Frontier data
        }

        result.data_size_bytes = size;
        result.entry_count = self.total_requests;

        // Calculate compression ratio (assume 40% compression with binary)
        result.compression_ratio = match request.format {
            ApiExportFormat::Json => 0,
            ApiExportFormat::Csv => 10,
            ApiExportFormat::Binary => 40,
        };

        result
    }

    /// Get dashboard state
    pub fn get_state(&self) -> DashboardState {
        self.state
    }

    /// Get HTTP request history
    pub fn get_request_history(&self) -> [Option<HttpRequest>; 100] {
        self.request_history
    }

    /// Get HTTP response history
    pub fn get_response_history(&self) -> [Option<HttpResponse>; 100] {
        self.response_history
    }

    /// Get connected client count
    pub fn client_count(&self) -> u16 {
        self.state.connected_clients
    }

    /// Get total requests processed
    pub fn total_requests(&self) -> u32 {
        self.total_requests
    }

    /// Get total WebSocket messages sent
    pub fn total_ws_messages(&self) -> u32 {
        self.total_ws_messages
    }

    /// Get pending command count
    pub fn pending_command_count(&self) -> usize {
        self.pending_commands.iter().filter(|s| s.is_some()).count()
    }

    /// Get health status
    pub fn health_status(&self) -> (bool, u8) {
        let healthy = self.state.evolution_active && !self.state.paused;
        let status_code = if healthy { 200 } else { 202 };  // 202 = Paused
        (healthy, status_code)
    }

    /// Get API statistics
    pub fn statistics(&self) -> (u32, u32, u16, u32) {
        (
            self.total_requests,
            self.total_ws_messages,
            self.state.connected_clients,
            self.pending_command_count() as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_enum() {
        assert_eq!(HttpMethod::Get as u8, 0);
        assert_eq!(HttpMethod::Post as u8, 1);
        assert_eq!(HttpMethod::Delete as u8, 3);
    }

    #[test]
    fn test_http_status_enum() {
        assert_eq!(HttpStatus::Ok.code(), 200);
        assert_eq!(HttpStatus::Created.code(), 201);
        assert_eq!(HttpStatus::InternalServerError.code(), 500);
    }

    #[test]
    fn test_http_status_is_success() {
        assert!(HttpStatus::Ok.is_success());
        assert!(HttpStatus::Created.is_success());
        assert!(!HttpStatus::BadRequest.is_success());
    }

    #[test]
    fn test_endpoint_type_enum() {
        assert_eq!(EndpointType::MetricsGet as u8, 0);
        assert_eq!(EndpointType::ControlPause as u8, 2);
        assert_eq!(EndpointType::FrontierQuery as u8, 8);
    }

    #[test]
    fn test_export_format_enum() {
        assert_eq!(ApiExportFormat::Json as u8, 0);
        assert_eq!(ApiExportFormat::Binary as u8, 2);
    }

    #[test]
    fn test_websocket_message_type_enum() {
        assert_eq!(WebSocketMessageType::MetricUpdate as u8, 0);
        assert_eq!(WebSocketMessageType::Error as u8, 3);
    }

    #[test]
    fn test_http_request_creation() {
        let req = HttpRequest::new(1, HttpMethod::Get, EndpointType::MetricsGet);
        assert_eq!(req.id, 1);
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.endpoint, EndpointType::MetricsGet);
    }

    #[test]
    fn test_http_request_with_query() {
        let req = HttpRequest::new(1, HttpMethod::Get, EndpointType::HistoryQuery).with_query(10);
        assert_eq!(req.query_param, 10);
    }

    #[test]
    fn test_http_response_creation() {
        let resp = HttpResponse::new(1, HttpStatus::Ok);
        assert_eq!(resp.request_id, 1);
        assert_eq!(resp.status, HttpStatus::Ok);
    }

    #[test]
    fn test_http_response_with_details() {
        let resp = HttpResponse::new(1, HttpStatus::Ok).with_size(256).with_time(50);
        assert_eq!(resp.size_bytes, 256);
        assert_eq!(resp.response_time_ms, 50);
    }

    #[test]
    fn test_websocket_message_creation() {
        let msg = WebSocketMessage::new(1, WebSocketMessageType::MetricUpdate);
        assert_eq!(msg.id, 1);
        assert_eq!(msg.msg_type, WebSocketMessageType::MetricUpdate);
    }

    #[test]
    fn test_websocket_message_with_size() {
        let msg = WebSocketMessage::new(1, WebSocketMessageType::MetricUpdate).with_size(512);
        assert_eq!(msg.payload_size, 512);
    }

    #[test]
    fn test_metric_snapshot_creation() {
        let snapshot = MetricSnapshot::new(1, 100, 75);
        assert_eq!(snapshot.total_mutations, 100);
        assert_eq!(snapshot.successful_mutations, 75);
    }

    #[test]
    fn test_metric_snapshot_success_rate() {
        let snapshot = MetricSnapshot::new(1, 100, 80);
        assert_eq!(snapshot.success_rate(), 80);
    }

    #[test]
    fn test_metric_snapshot_success_rate_zero() {
        let snapshot = MetricSnapshot::new(1, 0, 0);
        assert_eq!(snapshot.success_rate(), 0);
    }

    #[test]
    fn test_metric_snapshot_average_score() {
        let mut snapshot = MetricSnapshot::new(1, 100, 80);
        snapshot.reliability_score = 90;
        snapshot.power_efficiency_score = 80;
        snapshot.security_score = 70;
        let avg = snapshot.average_score();
        assert!(avg >= 79 && avg <= 81);  // Average of 90, 80, 70
    }

    #[test]
    fn test_dashboard_state_creation() {
        let state = DashboardState::new(1);
        assert!(state.evolution_active);
        assert!(!state.paused);
    }

    #[test]
    fn test_control_command_enum() {
        assert_eq!(ControlCommand::Pause as u8, 0);
        assert_eq!(ControlCommand::Stop as u8, 2);
        assert_eq!(ControlCommand::RecalculateFrontier as u8, 4);
    }

    #[test]
    fn test_control_request_creation() {
        let req = ControlRequest::new(1, ControlCommand::Pause);
        assert_eq!(req.id, 1);
        assert_eq!(req.command, ControlCommand::Pause);
    }

    #[test]
    fn test_control_request_with_parameter() {
        let req = ControlRequest::new(1, ControlCommand::ConfigChange).with_parameter(42);
        assert_eq!(req.parameter, 42);
    }

    #[test]
    fn test_control_response_creation() {
        let resp = ControlResponse::new(1, true);
        assert_eq!(resp.command_id, 1);
        assert!(resp.success);
    }

    #[test]
    fn test_export_request_creation() {
        let req = ApiExportRequest::new(1, ApiExportFormat::Json);
        assert_eq!(req.id, 1);
        assert_eq!(req.format, ApiExportFormat::Json);
        assert!(req.include_metrics);
    }

    #[test]
    fn test_export_result_creation() {
        let result = ApiExportResult::new(1, true);
        assert_eq!(result.id, 1);
        assert!(result.success);
    }

    #[test]
    fn test_dashboard_backend_creation() {
        let backend = DashboardBackend::new();
        assert_eq!(backend.total_requests(), 0);
        assert_eq!(backend.client_count(), 0);
    }

    #[test]
    fn test_dashboard_backend_update_metrics() {
        let mut backend = DashboardBackend::new();
        let snapshot = MetricSnapshot::new(1, 100, 80);
        backend.update_metrics(snapshot);
        assert_eq!(backend.state.metrics.total_mutations, 100);
    }

    #[test]
    fn test_dashboard_backend_handle_request() {
        let mut backend = DashboardBackend::new();
        let req = HttpRequest::new(1, HttpMethod::Get, EndpointType::MetricsGet);
        let resp = backend.handle_request(req);
        assert_eq!(resp.status, HttpStatus::Ok);
        assert_eq!(backend.total_requests(), 1);
    }

    #[test]
    fn test_dashboard_backend_bad_request() {
        let mut backend = DashboardBackend::new();
        let req = HttpRequest::new(1, HttpMethod::Post, EndpointType::MetricsGet);
        let resp = backend.handle_request(req);
        assert_eq!(resp.status, HttpStatus::BadRequest);
    }

    #[test]
    fn test_dashboard_backend_register_client() {
        let mut backend = DashboardBackend::new();
        assert!(backend.register_client(1));
        assert_eq!(backend.client_count(), 1);
    }

    #[test]
    fn test_dashboard_backend_unregister_client() {
        let mut backend = DashboardBackend::new();
        backend.register_client(1);
        assert!(backend.unregister_client(1));
        assert_eq!(backend.client_count(), 0);
    }

    #[test]
    fn test_dashboard_backend_broadcast_message() {
        let mut backend = DashboardBackend::new();
        backend.register_client(1);
        backend.register_client(2);
        let msg = WebSocketMessage::new(1, WebSocketMessageType::MetricUpdate);
        let count = backend.broadcast_message(msg);
        assert_eq!(count, 2);
        assert_eq!(backend.total_ws_messages(), 1);
    }

    #[test]
    fn test_dashboard_backend_queue_command() {
        let mut backend = DashboardBackend::new();
        let cmd = ControlRequest::new(1, ControlCommand::Pause);
        assert!(backend.queue_command(cmd));
        assert_eq!(backend.pending_command_count(), 1);
    }

    #[test]
    fn test_dashboard_backend_get_next_command() {
        let mut backend = DashboardBackend::new();
        let cmd = ControlRequest::new(1, ControlCommand::Pause);
        backend.queue_command(cmd);
        let retrieved = backend.get_next_command();
        assert!(retrieved.is_some());
        assert_eq!(backend.pending_command_count(), 0);
    }

    #[test]
    fn test_dashboard_backend_execute_pause() {
        let mut backend = DashboardBackend::new();
        let resp = backend.execute_command(ControlCommand::Pause);
        assert!(resp.success);
        assert!(backend.get_state().paused);
    }

    #[test]
    fn test_dashboard_backend_execute_resume() {
        let mut backend = DashboardBackend::new();
        backend.execute_command(ControlCommand::Pause);
        backend.execute_command(ControlCommand::Resume);
        assert!(!backend.get_state().paused);
    }

    #[test]
    fn test_dashboard_backend_execute_stop() {
        let mut backend = DashboardBackend::new();
        backend.execute_command(ControlCommand::Stop);
        assert!(!backend.get_state().evolution_active);
    }

    #[test]
    fn test_dashboard_backend_handle_export_json() {
        let backend = DashboardBackend::new();
        let req = ApiExportRequest::new(1, ApiExportFormat::Json);
        let result = backend.handle_export(req);
        assert!(result.success);
        assert_eq!(result.compression_ratio, 0);
    }

    #[test]
    fn test_dashboard_backend_handle_export_binary() {
        let backend = DashboardBackend::new();
        let req = ApiExportRequest::new(1, ApiExportFormat::Binary);
        let result = backend.handle_export(req);
        assert!(result.success);
        assert_eq!(result.compression_ratio, 40);
    }

    #[test]
    fn test_dashboard_backend_health_status() {
        let mut backend = DashboardBackend::new();
        let (healthy, code) = backend.health_status();
        assert!(healthy);
        assert_eq!(code, 200);

        backend.execute_command(ControlCommand::Pause);
        let (healthy, code) = backend.health_status();
        assert!(!healthy);
        assert_eq!(code, 202);
    }

    #[test]
    fn test_dashboard_backend_statistics() {
        let mut backend = DashboardBackend::new();
        let req = HttpRequest::new(1, HttpMethod::Get, EndpointType::MetricsGet);
        backend.handle_request(req);
        backend.register_client(1);

        let (reqs, msgs, clients, cmds) = backend.statistics();
        assert_eq!(reqs, 1);
        assert_eq!(clients, 1);
    }

    #[test]
    fn test_dashboard_backend_max_clients() {
        let mut backend = DashboardBackend::new();

        // Register 50 clients
        for i in 0..50 {
            assert!(backend.register_client(i));
        }

        // 51st should fail
        assert!(!backend.register_client(50));
    }

    #[test]
    fn test_export_with_history_and_frontier() {
        let backend = DashboardBackend::new();
        let mut req = ApiExportRequest::new(1, ApiExportFormat::Json);
        req.include_history = true;
        req.include_frontier = true;
        let result = backend.handle_export(req);
        assert!(result.success);
        assert!(result.data_size_bytes > 128);  // More than just metrics
    }

    #[test]
    fn test_metric_snapshot_max_scores() {
        let mut snapshot = MetricSnapshot::new(1, 100, 100);
        snapshot.performance_gain_percent = 50;
        snapshot.reliability_score = 100;
        snapshot.power_efficiency_score = 100;
        snapshot.security_score = 100;
        assert_eq!(snapshot.average_score(), 100);
    }

    #[test]
    fn test_request_response_pairing() {
        let mut backend = DashboardBackend::new();

        let req1 = HttpRequest::new(1, HttpMethod::Get, EndpointType::MetricsGet);
        let req2 = HttpRequest::new(2, HttpMethod::Get, EndpointType::HealthCheck);

        let resp1 = backend.handle_request(req1);
        let resp2 = backend.handle_request(req2);

        assert_eq!(resp1.request_id, 1);
        assert_eq!(resp2.request_id, 2);
        assert_eq!(backend.total_requests(), 2);
    }

    #[test]
    fn test_multiple_commands_queue() {
        let mut backend = DashboardBackend::new();

        let cmd1 = ControlRequest::new(1, ControlCommand::Pause);
        let cmd2 = ControlRequest::new(2, ControlCommand::ConfigChange);

        assert!(backend.queue_command(cmd1));
        assert!(backend.queue_command(cmd2));
        assert_eq!(backend.pending_command_count(), 2);

        let retrieved1 = backend.get_next_command();
        assert!(retrieved1.is_some());
        assert_eq!(backend.pending_command_count(), 1);
    }
}
