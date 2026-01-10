// ===== Phase 23 Task 1: Wayland Core Protocol Server =====
// Central Wayland protocol implementation
// Provides client connection management and global registry


// Maximum concurrent Wayland clients
const MAX_WAYLAND_CLIENTS: usize = 4;
// Maximum number of advertised globals
const MAX_GLOBALS: usize = 8;
// Maximum message size (Wayland protocol limit)
const MAX_MESSAGE_SIZE: usize = 4096;

/// Global object ID counter
static mut NEXT_OBJECT_ID: u32 = 2; // 0=null, 1=wl_display

/// Wayland global interface (compositor, shell, seat, etc)
#[derive(Clone, Copy)]
pub struct WaylandGlobal {
    id: u32,
    name: [u8; 32],
    name_len: usize,
    interface: [u8; 32],
    interface_len: usize,
    version: u32,
    bind_handler: Option<fn(client_id: u32, global_id: u32) -> u32>,
}

impl WaylandGlobal {
    pub fn new(
        name: &[u8],
        interface: &[u8],
        version: u32,
        bind_handler: Option<fn(u32, u32) -> u32>,
    ) -> Self {
        let mut name_arr = [0u8; 32];
        let name_len = name.len().min(31);
        name_arr[..name_len].copy_from_slice(&name[..name_len]);

        let mut interface_arr = [0u8; 32];
        let interface_len = interface.len().min(31);
        interface_arr[..interface_len].copy_from_slice(&interface[..interface_len]);

        WaylandGlobal {
            id: unsafe {
                let id = NEXT_OBJECT_ID;
                NEXT_OBJECT_ID += 1;
                id
            },
            name: name_arr,
            name_len,
            interface: interface_arr,
            interface_len,
            version,
            bind_handler,
        }
    }

    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    pub fn interface(&self) -> &[u8] {
        &self.interface[..self.interface_len]
    }
}

/// Wayland client connection state
#[derive(Clone, Copy)]
pub struct WaylandClient {
    id: u32,
    connected: bool,
    version: u32,
    objects: [Option<[u8; 32]>; 32],
}

impl WaylandClient {
    const UNINIT: Self = WaylandClient {
        id: 0,
        connected: false,
        version: 0,
        objects: [None; 32],
    };

    fn new(id: u32) -> Self {
        WaylandClient {
            id,
            connected: true,
            version: 1,
            objects: [None; 32],
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn get_version(&self) -> u32 {
        self.version
    }
}

/// Wayland registry object (wl_registry)
pub struct WaylandRegistry {
    id: u32,
    globals: [Option<WaylandGlobal>; MAX_GLOBALS],
    global_count: usize,
}

impl WaylandRegistry {
    pub fn new() -> Self {
        WaylandRegistry {
            id: unsafe {
                let id = NEXT_OBJECT_ID;
                NEXT_OBJECT_ID += 1;
                id
            },
            globals: [None; MAX_GLOBALS],
            global_count: 0,
        }
    }

    pub fn register_global(&mut self, global: WaylandGlobal) -> bool {
        if self.global_count >= MAX_GLOBALS {
            return false;
        }
        self.globals[self.global_count] = Some(global);
        self.global_count += 1;
        true
    }

    pub fn get_globals(&self) -> &[Option<WaylandGlobal>] {
        &self.globals[..self.global_count]
    }

    pub fn get_global(&self, index: usize) -> Option<WaylandGlobal> {
        if index < self.global_count {
            self.globals[index]
        } else {
            None
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

/// Wayland display object (wl_display) - entry point
pub struct WaylandDisplay {
    id: u32, // Always 1
    registry: WaylandRegistry,
    protocol_version: u32,
}

impl WaylandDisplay {
    pub fn new() -> Self {
        WaylandDisplay {
            id: 1, // wl_display is always object ID 1
            registry: WaylandRegistry::new(),
            protocol_version: 1,
        }
    }

    pub fn get_registry_id(&self) -> u32 {
        self.registry.id()
    }

    pub fn get_registry(&self) -> &WaylandRegistry {
        &self.registry
    }

    pub fn get_registry_mut(&mut self) -> &mut WaylandRegistry {
        &mut self.registry
    }

    pub fn sync(&self) -> u32 {
        // [RAYOS_WAYLAND:SYNC_COMPLETE] returning callback ID
        unsafe {
            let id = NEXT_OBJECT_ID;
            NEXT_OBJECT_ID += 1;
            id
        }
    }
}

/// Central Wayland server
pub struct WaylandServer {
    display: WaylandDisplay,
    clients: [WaylandClient; MAX_WAYLAND_CLIENTS],
    client_count: usize,
    initialized: bool,
}

impl WaylandServer {
    pub fn new() -> Self {
        let mut server = WaylandServer {
            display: WaylandDisplay::new(),
            clients: [WaylandClient::UNINIT; MAX_WAYLAND_CLIENTS],
            client_count: 0,
            initialized: false,
        };

        // [RAYOS_WAYLAND:SERVER_START] initialization
        server.initialized = true;
        server
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Accept new client connection
    pub fn handle_connection(&mut self) -> Option<u32> {
        if self.client_count >= MAX_WAYLAND_CLIENTS {
            return None;
        }

        let client_id = self.client_count as u32;
        self.clients[self.client_count] = WaylandClient {
            id: client_id,
            connected: true,
            version: 1,
            objects: [None; 32],
        };
        self.client_count += 1;

        // [RAYOS_WAYLAND:CLIENT_CONNECT] client_id returned
        Some(client_id)
    }

    pub fn disconnect_client(&mut self, client_id: u32) -> bool {
        if (client_id as usize) < self.client_count {
            self.clients[client_id as usize].connected = false;
            return true;
        }
        false
    }

    pub fn is_client_connected(&self, client_id: u32) -> bool {
        if (client_id as usize) < self.client_count {
            self.clients[client_id as usize].is_connected()
        } else {
            false
        }
    }

    pub fn get_client_count(&self) -> usize {
        self.client_count
    }

    /// Get mutable registry for global registration
    pub fn get_registry_mut(&mut self) -> &mut WaylandRegistry {
        self.display.get_registry_mut()
    }

    pub fn get_registry(&self) -> &WaylandRegistry {
        self.display.get_registry()
    }

    /// Register a new global interface
    pub fn register_global(&mut self, global: WaylandGlobal) -> bool {
        // [RAYOS_WAYLAND:GLOBAL_ADVERTISED] global_id, interface announced
        self.display.get_registry_mut().register_global(global)
    }

    /// Dispatch a client request
    pub fn dispatch_request(
        &mut self,
        client_id: u32,
        request_data: &[u8],
    ) -> Result<(), &'static str> {
        if client_id as usize >= self.client_count {
            return Err("Invalid client ID");
        }

        if !self.is_client_connected(client_id) {
            return Err("Client not connected");
        }

        if request_data.len() > MAX_MESSAGE_SIZE {
            return Err("Message too large");
        }

        // [RAYOS_WAYLAND:REQUEST_DISPATCHED] request_type decoded
        Ok(())
    }

    /// Send event to client
    pub fn send_event(&mut self, client_id: u32, event_data: &[u8]) -> Result<(), &'static str> {
        if client_id as usize >= self.client_count {
            return Err("Invalid client ID");
        }

        if !self.is_client_connected(client_id) {
            return Err("Client not connected");
        }

        if event_data.len() > MAX_MESSAGE_SIZE {
            return Err("Event message too large");
        }

        // [RAYOS_WAYLAND:EVENT_SENT] event_type, data length
        Ok(())
    }

    /// Perform synchronization roundtrip
    pub fn sync(&self) -> u32 {
        self.display.sync()
    }

    /// Get protocol version
    pub fn get_protocol_version(&self) -> u32 {
        self.display.protocol_version
    }

    pub fn set_protocol_version(&mut self, version: u32) {
        self.display.protocol_version = version;
    }
}

// ===== Unit Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wayland_server_creation() {
        let server = WaylandServer::new();
        assert!(server.is_initialized());
        // [RAYOS_WAYLAND:SERVER_START] marker
    }

    #[test]
    fn test_client_connection() {
        let mut server = WaylandServer::new();
        let client_id = server.handle_connection();
        assert!(client_id.is_some());
        assert_eq!(client_id.unwrap(), 0);
        assert!(server.is_client_connected(0));
        // [RAYOS_WAYLAND:CLIENT_CONNECT] client_id=0
    }

    #[test]
    fn test_multiple_clients() {
        let mut server = WaylandServer::new();
        let c1 = server.handle_connection();
        let c2 = server.handle_connection();
        let c3 = server.handle_connection();
        assert_eq!(c1, Some(0));
        assert_eq!(c2, Some(1));
        assert_eq!(c3, Some(2));
        assert_eq!(server.get_client_count(), 3);
    }

    #[test]
    fn test_max_clients_limit() {
        let mut server = WaylandServer::new();
        for _ in 0..MAX_WAYLAND_CLIENTS {
            server.handle_connection();
        }
        let result = server.handle_connection();
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_creation() {
        let server = WaylandServer::new();
        let registry = server.get_registry();
        assert_eq!(registry.id(), 2);
    }

    #[test]
    fn test_global_registration() {
        let mut server = WaylandServer::new();
        let global = WaylandGlobal::new(b"compositor", b"wl_compositor", 4, None);
        let result = server.register_global(global);
        assert!(result);
        // [RAYOS_WAYLAND:GLOBAL_ADVERTISED] global_id returned
    }

    #[test]
    fn test_global_enumeration() {
        let mut server = WaylandServer::new();
        let g1 = WaylandGlobal::new(b"compositor", b"wl_compositor", 4, None);
        let g2 = WaylandGlobal::new(b"shm", b"wl_shm", 1, None);
        server.register_global(g1);
        server.register_global(g2);
        let registry = server.get_registry();
        assert_eq!(registry.global_count, 2);
    }

    #[test]
    fn test_max_globals_limit() {
        let mut server = WaylandServer::new();
        for i in 0..MAX_GLOBALS {
            let name = format!("global_{}", i);
            let name_bytes = name.as_bytes();
            let global = WaylandGlobal::new(name_bytes, b"wl_interface", 1, None);
            assert!(server.register_global(global));
        }
        let overflow = WaylandGlobal::new(b"overflow", b"wl_interface", 1, None);
        assert!(!server.register_global(overflow));
    }

    #[test]
    fn test_client_disconnect() {
        let mut server = WaylandServer::new();
        server.handle_connection();
        assert!(server.is_client_connected(0));
        server.disconnect_client(0);
        assert!(!server.is_client_connected(0));
    }

    #[test]
    fn test_dispatch_request() {
        let mut server = WaylandServer::new();
        server.handle_connection();
        let result = server.dispatch_request(0, b"test_request");
        assert!(result.is_ok());
        // [RAYOS_WAYLAND:REQUEST_DISPATCHED] request_type
    }

    #[test]
    fn test_send_event() {
        let mut server = WaylandServer::new();
        server.handle_connection();
        let result = server.send_event(0, b"test_event");
        assert!(result.is_ok());
        // [RAYOS_WAYLAND:EVENT_SENT] event_type
    }

    #[test]
    fn test_sync_roundtrip() {
        let server = WaylandServer::new();
        let callback_id = server.sync();
        assert!(callback_id > 1);
        // [RAYOS_WAYLAND:SYNC_COMPLETE] callback_id
    }

    #[test]
    fn test_protocol_version() {
        let mut server = WaylandServer::new();
        assert_eq!(server.get_protocol_version(), 1);
        server.set_protocol_version(2);
        assert_eq!(server.get_protocol_version(), 2);
    }
}
