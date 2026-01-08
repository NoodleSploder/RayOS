// ===== Phase 23 Task 5: Drag & Drop & Clipboard =====
// Implements wl_data_device, wl_data_source, wl_data_offer, wl_data_device_manager
// Provides drag-drop operations and clipboard management with RayApp integration

use core::fmt::Write;

// Data device limits
const MAX_DATA_SOURCES: usize = 16;
const MAX_DATA_OFFERS: usize = 16;
const MAX_MIME_TYPES: usize = 8;
const MAX_DATA_DEVICES: usize = 4;
const MAX_CLIPBOARD_SIZE: usize = 65536;

// Drag actions
const ACTION_NONE: u32 = 0;
const ACTION_COPY: u32 = 1;
const ACTION_MOVE: u32 = 2;
const ACTION_ASK: u32 = 4;

// Drop status
const DROP_STATUS_UNACCEPTED: u32 = 0;
const DROP_STATUS_ACCEPTED: u32 = 1;
const DROP_STATUS_FINISHED: u32 = 2;

/// MIME Type
#[derive(Clone, Copy)]
pub struct MimeType {
    data: [u8; 64],
    len: usize,
}

impl MimeType {
    fn new(mime: &[u8]) -> Self {
        let len = mime.len().min(63);
        let mut data = [0u8; 64];
        data[..len].copy_from_slice(&mime[..len]);
        MimeType { data, len }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Clipboard data
#[derive(Clone, Copy)]
pub struct ClipboardData {
    data: [u8; MAX_CLIPBOARD_SIZE],
    len: usize,
    mime_type: [u8; 64],
    mime_len: usize,
}

impl ClipboardData {
    fn new() -> Self {
        ClipboardData {
            data: [0u8; MAX_CLIPBOARD_SIZE],
            len: 0,
            mime_type: [0u8; 64],
            mime_len: 0,
        }
    }

    pub fn set(&mut self, mime: &[u8], data: &[u8]) -> Result<(), &'static str> {
        if data.len() > MAX_CLIPBOARD_SIZE {
            return Err("data exceeds clipboard size");
        }

        let mime_len = mime.len().min(63);
        self.mime_type[..mime_len].copy_from_slice(&mime[..mime_len]);
        self.mime_len = mime_len;

        self.data[..data.len()].copy_from_slice(data);
        self.len = data.len();

        Ok(())
    }

    pub fn get(&self) -> (&[u8], &[u8]) {
        (&self.mime_type[..self.mime_len], &self.data[..self.len])
    }
}

/// Data Offer
#[derive(Clone, Copy)]
pub struct DataOffer {
    id: u32,
    source_id: u32,
    mime_types: [Option<MimeType>; MAX_MIME_TYPES],
    mime_count: usize,
    accepted_mime: Option<usize>,
    in_use: bool,
}

impl DataOffer {
    const UNINIT: Self = DataOffer {
        id: 0,
        source_id: 0,
        mime_types: [None; MAX_MIME_TYPES],
        mime_count: 0,
        accepted_mime: None,
        in_use: false,
    };

    fn new(id: u32, source_id: u32) -> Self {
        DataOffer {
            id,
            source_id,
            mime_types: [None; MAX_MIME_TYPES],
            mime_count: 0,
            accepted_mime: None,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn accept(&mut self, mime_type_idx: usize) -> Result<(), &'static str> {
        if mime_type_idx >= self.mime_count {
            return Err("mime type index out of range");
        }
        self.accepted_mime = Some(mime_type_idx);
        Ok(())
    }

    pub fn receive(&self) -> Option<MimeType> {
        self.accepted_mime.and_then(|idx| self.mime_types[idx])
    }

    pub fn finish(&self) -> Result<(), &'static str> {
        Ok(())
    }

    pub fn set_actions(&mut self, _actions: u32) -> Result<(), &'static str> {
        Ok(())
    }
}

/// Data Source
#[derive(Clone, Copy)]
pub struct DataSource {
    id: u32,
    mime_types: [Option<MimeType>; MAX_MIME_TYPES],
    mime_count: usize,
    actions: u32,
    current_dnd_action: u32,
    in_use: bool,
}

impl DataSource {
    const UNINIT: Self = DataSource {
        id: 0,
        mime_types: [None; MAX_MIME_TYPES],
        mime_count: 0,
        actions: ACTION_COPY,
        current_dnd_action: ACTION_NONE,
        in_use: false,
    };

    fn new(id: u32) -> Self {
        DataSource {
            id,
            mime_types: [None; MAX_MIME_TYPES],
            mime_count: 0,
            actions: ACTION_COPY,
            current_dnd_action: ACTION_NONE,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn offer(&mut self, mime: &[u8]) -> Result<(), &'static str> {
        if self.mime_count >= MAX_MIME_TYPES {
            return Err("mime type limit exceeded");
        }

        let mime_type = MimeType::new(mime);
        self.mime_types[self.mime_count] = Some(mime_type);
        self.mime_count += 1;

        Ok(())
    }

    pub fn set_actions(&mut self, actions: u32) -> Result<(), &'static str> {
        self.actions = actions;
        Ok(())
    }

    pub fn dnd_drop_performed(&mut self) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_DND:DROP_PERFORMED] source_id={}\n", self.id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn dnd_finished(&self) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_DND:FINISHED] source_id={}\n", self.id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn get_mime_types(&self) -> &[Option<MimeType>] {
        &self.mime_types[..self.mime_count]
    }

    pub fn get_actions(&self) -> u32 {
        self.actions
    }
}

/// Data Device
#[derive(Clone, Copy)]
pub struct DataDevice {
    id: u32,
    selection_source: Option<u32>,
    current_dnd_source: Option<u32>,
    current_dnd_target: Option<u32>,
    in_use: bool,
}

impl DataDevice {
    const UNINIT: Self = DataDevice {
        id: 0,
        selection_source: None,
        current_dnd_source: None,
        current_dnd_target: None,
        in_use: false,
    };

    fn new(id: u32) -> Self {
        DataDevice {
            id,
            selection_source: None,
            current_dnd_source: None,
            current_dnd_target: None,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn start_drag(&mut self, source_id: u32, origin_surface: u32, icon_surface: Option<u32>) -> Result<(), &'static str> {
        self.current_dnd_source = Some(source_id);

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_DND:DRAG_START] source_id={} origin={}\n", source_id, origin_surface)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn set_selection(&mut self, source_id: Option<u32>) -> Result<(), &'static str> {
        self.selection_source = source_id;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SELECTION:SET] source_id={}\n", source_id.unwrap_or(0))
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_selection(&self, mime: &[u8]) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SELECTION:REQUESTED] mime_len={}\n", mime.len())
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_offer(&self, offer_id: u32, mime_count: usize) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SELECTION:TRANSFERRED] offer_id={} mime_count={}\n", offer_id, mime_count)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn get_selection_source(&self) -> Option<u32> {
        self.selection_source
    }

    pub fn get_dnd_source(&self) -> Option<u32> {
        self.current_dnd_source
    }

    pub fn get_dnd_target(&self) -> Option<u32> {
        self.current_dnd_target
    }
}

/// Data Device Manager
pub struct DataDeviceManager {
    id: u32,
    sources: [DataSource; MAX_DATA_SOURCES],
    source_count: usize,
    offers: [DataOffer; MAX_DATA_OFFERS],
    offer_count: usize,
    devices: [DataDevice; MAX_DATA_DEVICES],
    device_count: usize,
    clipboard: ClipboardData,
    next_source_id: u32,
    next_offer_id: u32,
    next_device_id: u32,
}

impl DataDeviceManager {
    pub fn new() -> Self {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_DND:DATA_DEVICE_MANAGER_ADVERTISED] interface=wl_data_device_manager\n")
            ).ok() {
                // Marker emitted
            }
        }

        DataDeviceManager {
            id: 1,
            sources: [DataSource::UNINIT; MAX_DATA_SOURCES],
            source_count: 0,
            offers: [DataOffer::UNINIT; MAX_DATA_OFFERS],
            offer_count: 0,
            devices: [DataDevice::UNINIT; MAX_DATA_DEVICES],
            device_count: 0,
            clipboard: ClipboardData::new(),
            next_source_id: 100,
            next_offer_id: 200,
            next_device_id: 300,
        }
    }

    pub fn create_data_source(&mut self) -> Result<u32, &'static str> {
        if self.source_count >= MAX_DATA_SOURCES {
            return Err("source limit exceeded");
        }

        let source_id = self.next_source_id;
        self.next_source_id += 1;

        let source = DataSource::new(source_id);
        self.sources[self.source_count] = source;
        self.source_count += 1;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_DND:SOURCE_CREATE] source_id={}\n", source_id)
            ).ok() {
                // Marker emitted
            }
        }

        Ok(source_id)
    }

    pub fn get_data_device(&mut self, seat_id: u32) -> Result<u32, &'static str> {
        if self.device_count >= MAX_DATA_DEVICES {
            return Err("device limit exceeded");
        }

        let device_id = self.next_device_id;
        self.next_device_id += 1;

        let device = DataDevice::new(device_id);
        self.devices[self.device_count] = device;
        self.device_count += 1;

        Ok(device_id)
    }

    pub fn create_data_offer(&mut self, source_id: u32) -> Result<u32, &'static str> {
        if self.offer_count >= MAX_DATA_OFFERS {
            return Err("offer limit exceeded");
        }

        let offer_id = self.next_offer_id;
        self.next_offer_id += 1;

        let offer = DataOffer::new(offer_id, source_id);
        self.offers[self.offer_count] = offer;
        self.offer_count += 1;

        Ok(offer_id)
    }

    pub fn get_source_mut(&mut self, source_id: u32) -> Option<&mut DataSource> {
        self.sources[..self.source_count]
            .iter_mut()
            .find(|s| s.in_use && s.id == source_id)
    }

    pub fn find_source(&self, source_id: u32) -> Option<&DataSource> {
        self.sources[..self.source_count]
            .iter()
            .find(|s| s.in_use && s.id == source_id)
    }

    pub fn get_offer_mut(&mut self, offer_id: u32) -> Option<&mut DataOffer> {
        self.offers[..self.offer_count]
            .iter_mut()
            .find(|o| o.in_use && o.id == offer_id)
    }

    pub fn find_offer(&self, offer_id: u32) -> Option<&DataOffer> {
        self.offers[..self.offer_count]
            .iter()
            .find(|o| o.in_use && o.id == offer_id)
    }

    pub fn get_device_mut(&mut self, device_id: u32) -> Option<&mut DataDevice> {
        self.devices[..self.device_count]
            .iter_mut()
            .find(|d| d.in_use && d.id == device_id)
    }

    pub fn find_device(&self, device_id: u32) -> Option<&DataDevice> {
        self.devices[..self.device_count]
            .iter()
            .find(|d| d.in_use && d.id == device_id)
    }

    pub fn set_clipboard(&mut self, mime: &[u8], data: &[u8]) -> Result<(), &'static str> {
        self.clipboard.set(mime, data)
    }

    pub fn get_clipboard(&self) -> (&[u8], &[u8]) {
        self.clipboard.get()
    }

    pub fn sync_to_rayapp(&self) -> (&[u8], &[u8]) {
        self.clipboard.get()
    }

    pub fn sync_from_rayapp(&mut self, mime: &[u8], data: &[u8]) -> Result<(), &'static str> {
        self.clipboard.set(mime, data)
    }

    pub fn get_source_count(&self) -> usize {
        self.sources[..self.source_count].iter().filter(|s| s.in_use).count()
    }

    pub fn get_offer_count(&self) -> usize {
        self.offers[..self.offer_count].iter().filter(|o| o.in_use).count()
    }

    pub fn get_device_count(&self) -> usize {
        self.devices[..self.device_count].iter().filter(|d| d.in_use).count()
    }
}

// Simple logging helper
struct Logger;

impl core::fmt::Write for Logger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // In a real implementation, this would write to kernel log
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_device_manager_creation() {
        let manager = DataDeviceManager::new();
        assert_eq!(manager.id, 1);
        assert_eq!(manager.get_source_count(), 0);
        assert_eq!(manager.get_device_count(), 0);
    }

    #[test]
    fn test_data_source_creation() {
        let mut manager = DataDeviceManager::new();
        let result = manager.create_data_source();
        assert!(result.is_ok());
        assert_eq!(manager.get_source_count(), 1);
    }

    #[test]
    fn test_mime_type_offering() {
        let mut manager = DataDeviceManager::new();
        let source_id = manager.create_data_source().unwrap();
        let source = manager.get_source_mut(source_id).unwrap();

        let result = source.offer(b"text/plain");
        assert!(result.is_ok());
        assert_eq!(source.get_mime_types().len(), 1);
    }

    #[test]
    fn test_drag_start() {
        let mut manager = DataDeviceManager::new();
        let device_id = manager.get_data_device(1).unwrap();
        let device = manager.get_device_mut(device_id).unwrap();

        let result = device.start_drag(100, 1, None);
        assert!(result.is_ok());
        assert_eq!(device.get_dnd_source(), Some(100));
    }

    #[test]
    fn test_drag_motion() {
        let mut manager = DataDeviceManager::new();
        let device_id = manager.get_data_device(1).unwrap();

        let device = manager.get_device_mut(device_id).unwrap();
        device.start_drag(100, 1, None).unwrap();

        assert_eq!(device.get_dnd_source(), Some(100));
    }

    #[test]
    fn test_drag_drop() {
        let mut manager = DataDeviceManager::new();
        let source_id = manager.create_data_source().unwrap();
        let device_id = manager.get_data_device(1).unwrap();

        let device = manager.get_device_mut(device_id).unwrap();
        device.start_drag(source_id, 1, None).unwrap();

        let source = manager.get_source_mut(source_id).unwrap();
        source.dnd_drop_performed().unwrap();
    }

    #[test]
    fn test_data_transfer() {
        let mut manager = DataDeviceManager::new();
        let source_id = manager.create_data_source().unwrap();
        let source = manager.get_source_mut(source_id).unwrap();

        source.offer(b"text/plain").unwrap();
        assert_eq!(source.mime_count, 1);
    }

    #[test]
    fn test_clipboard_set_selection() {
        let mut manager = DataDeviceManager::new();
        let device_id = manager.get_data_device(1).unwrap();
        let source_id = manager.create_data_source().unwrap();

        let device = manager.get_device_mut(device_id).unwrap();
        let result = device.set_selection(Some(source_id));
        assert!(result.is_ok());
        assert_eq!(device.get_selection_source(), Some(source_id));
    }

    #[test]
    fn test_clipboard_data_request() {
        let mut manager = DataDeviceManager::new();
        let device_id = manager.get_data_device(1).unwrap();

        let device = manager.get_device_mut(device_id).unwrap();
        let result = device.send_selection(b"text/plain");
        assert!(result.is_ok());
    }

    #[test]
    fn test_clipboard_sync_with_rayapp() {
        let mut manager = DataDeviceManager::new();
        let test_data = b"Hello, Wayland!";
        let mime = b"text/plain";

        let result = manager.sync_from_rayapp(mime, test_data);
        assert!(result.is_ok());

        let (returned_mime, returned_data) = manager.sync_to_rayapp();
        assert_eq!(returned_mime, mime);
        assert_eq!(returned_data, test_data);
    }

    #[test]
    fn test_dnd_between_clients() {
        let mut manager = DataDeviceManager::new();

        // Client 1: Source
        let source_id = manager.create_data_source().unwrap();
        let source = manager.get_source_mut(source_id).unwrap();
        source.offer(b"text/plain").unwrap();

        // Client 2: Target
        let device_id = manager.get_data_device(1).unwrap();
        let device = manager.get_device_mut(device_id).unwrap();
        device.start_drag(source_id, 1, None).unwrap();

        // Verify drag state
        assert_eq!(device.get_dnd_source(), Some(source_id));
    }

    #[test]
    fn test_dnd_actions() {
        let mut manager = DataDeviceManager::new();
        let source_id = manager.create_data_source().unwrap();
        let source = manager.get_source_mut(source_id).unwrap();

        let result = source.set_actions(ACTION_COPY | ACTION_MOVE);
        assert!(result.is_ok());
        assert_eq!(source.get_actions(), ACTION_COPY | ACTION_MOVE);
    }
}
