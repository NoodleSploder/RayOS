//! VM Guest Data Bridge for RayOS UI
//!
//! Virtio-based data exchange between host and VM guests.
//!
//! # Overview
//!
//! The VM Data Bridge provides:
//! - Virtio clipboard device support
//! - Guest-to-host and host-to-guest sync
//! - Drag-drop across VM boundaries
//! - File transfer support
//! - Secure data validation
//!
//! # Markers
//!
//! - `RAYOS_VMBRIDGE:CONNECTED` - Guest connected
//! - `RAYOS_VMBRIDGE:DISCONNECTED` - Guest disconnected
//! - `RAYOS_VMBRIDGE:SYNC_H2G` - Host to guest sync
//! - `RAYOS_VMBRIDGE:SYNC_G2H` - Guest to host sync
//! - `RAYOS_VMBRIDGE:TRANSFER` - Data transfer completed

use super::clipboard::{ClipboardEntry, ClipboardFormat, ClipboardSelection};
use super::drag_drop::{DragAction, DragPayload};
use super::data_transfer::FormatId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum VM guests.
pub const MAX_VM_GUESTS: usize = 8;

/// Maximum pending transfers.
pub const MAX_PENDING_TRANSFERS: usize = 16;

/// Maximum transfer buffer size.
pub const MAX_TRANSFER_SIZE: usize = 1024 * 1024; // 1MB

/// Virtio queue size.
pub const VIRTQ_SIZE: usize = 64;

/// Virtio feature: clipboard support.
pub const VIRTIO_CLIPBOARD_F_COPY: u32 = 1 << 0;
pub const VIRTIO_CLIPBOARD_F_PASTE: u32 = 1 << 1;
pub const VIRTIO_CLIPBOARD_F_DRAGDROP: u32 = 1 << 2;
pub const VIRTIO_CLIPBOARD_F_FILES: u32 = 1 << 3;

// ============================================================================
// Bridge State
// ============================================================================

/// Bridge connection state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BridgeState {
    /// Not connected.
    Disconnected = 0,
    /// Connecting.
    Connecting = 1,
    /// Connected and ready.
    Connected = 2,
    /// Syncing data.
    Syncing = 3,
    /// Error state.
    Error = 4,
}

impl Default for BridgeState {
    fn default() -> Self {
        BridgeState::Disconnected
    }
}

// ============================================================================
// Virtio Message Types
// ============================================================================

/// Virtio clipboard message type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VirtioMsgType {
    /// Request format list.
    GetFormats = 1,
    /// Response with format list.
    Formats = 2,
    /// Request data for format.
    GetData = 3,
    /// Response with data.
    Data = 4,
    /// Set clipboard content.
    SetData = 5,
    /// Clear clipboard.
    Clear = 6,
    /// Drag started.
    DragStart = 10,
    /// Drag update.
    DragUpdate = 11,
    /// Drop occurred.
    DragDrop = 12,
    /// Drag cancelled.
    DragCancel = 13,
    /// File transfer start.
    FileStart = 20,
    /// File transfer chunk.
    FileChunk = 21,
    /// File transfer complete.
    FileComplete = 22,
    /// Error response.
    Error = 255,
}

impl From<u8> for VirtioMsgType {
    fn from(v: u8) -> Self {
        match v {
            1 => VirtioMsgType::GetFormats,
            2 => VirtioMsgType::Formats,
            3 => VirtioMsgType::GetData,
            4 => VirtioMsgType::Data,
            5 => VirtioMsgType::SetData,
            6 => VirtioMsgType::Clear,
            10 => VirtioMsgType::DragStart,
            11 => VirtioMsgType::DragUpdate,
            12 => VirtioMsgType::DragDrop,
            13 => VirtioMsgType::DragCancel,
            20 => VirtioMsgType::FileStart,
            21 => VirtioMsgType::FileChunk,
            22 => VirtioMsgType::FileComplete,
            _ => VirtioMsgType::Error,
        }
    }
}

// ============================================================================
// Virtio Message Header
// ============================================================================

/// Virtio clipboard message header.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct VirtioMsgHeader {
    /// Message type.
    pub msg_type: u8,
    /// Selection type (primary/clipboard).
    pub selection: u8,
    /// Format ID.
    pub format: u16,
    /// Data length.
    pub length: u32,
    /// Sequence number.
    pub sequence: u32,
    /// Flags.
    pub flags: u32,
}

impl VirtioMsgHeader {
    /// Create new header.
    pub fn new(msg_type: VirtioMsgType, selection: ClipboardSelection, length: u32) -> Self {
        Self {
            msg_type: msg_type as u8,
            selection: selection as u8,
            format: 0,
            length,
            sequence: 0,
            flags: 0,
        }
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0] = self.msg_type;
        bytes[1] = self.selection;
        bytes[2..4].copy_from_slice(&self.format.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.length.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.flags.to_le_bytes());
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        Some(Self {
            msg_type: bytes[0],
            selection: bytes[1],
            format: u16::from_le_bytes([bytes[2], bytes[3]]),
            length: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            sequence: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            flags: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        })
    }

    /// Get message type.
    pub fn get_type(&self) -> VirtioMsgType {
        VirtioMsgType::from(self.msg_type)
    }

    /// Get selection.
    pub fn get_selection(&self) -> ClipboardSelection {
        if self.selection == 0 {
            ClipboardSelection::Primary
        } else {
            ClipboardSelection::Clipboard
        }
    }
}

// ============================================================================
// Transfer State
// ============================================================================

/// Pending transfer state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TransferState {
    /// Idle/not in use.
    Idle = 0,
    /// Pending - waiting for guest response.
    Pending = 1,
    /// In progress.
    InProgress = 2,
    /// Completed successfully.
    Completed = 3,
    /// Failed.
    Failed = 4,
}

impl Default for TransferState {
    fn default() -> Self {
        TransferState::Idle
    }
}

/// Pending transfer tracking.
#[derive(Clone, Copy)]
pub struct PendingTransfer {
    /// Transfer ID.
    pub id: u32,
    /// Sequence number.
    pub sequence: u32,
    /// VM ID.
    pub vm_id: u32,
    /// State.
    pub state: TransferState,
    /// Request type.
    pub msg_type: u8,
    /// Format requested.
    pub format: FormatId,
    /// Bytes transferred.
    pub bytes_transferred: usize,
    /// Total bytes expected.
    pub total_bytes: usize,
    /// Start timestamp.
    pub start_time: u64,
    /// Timeout (ticks).
    pub timeout: u64,
}

impl PendingTransfer {
    /// Create empty transfer.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            sequence: 0,
            vm_id: 0,
            state: TransferState::Idle,
            msg_type: 0,
            format: 0,
            bytes_transferred: 0,
            total_bytes: 0,
            start_time: 0,
            timeout: 1000, // Default 1000 ticks
        }
    }

    /// Check if active.
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            TransferState::Pending | TransferState::InProgress
        )
    }

    /// Check if timed out.
    pub fn is_timed_out(&self, current_time: u64) -> bool {
        current_time - self.start_time > self.timeout
    }

    /// Get progress (0-100).
    pub fn progress(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.bytes_transferred * 100) / self.total_bytes).min(100) as u8
    }
}

// ============================================================================
// VM Guest
// ============================================================================

/// VM guest state.
#[derive(Clone, Copy)]
pub struct VmGuest {
    /// VM ID.
    pub id: u32,
    /// Connection state.
    pub state: BridgeState,
    /// Supported features.
    pub features: u32,
    /// Last activity timestamp.
    pub last_activity: u64,
    /// Sequence counter.
    pub sequence: u32,
    /// Clipboard formats available.
    pub clipboard_formats: [FormatId; 8],
    /// Format count.
    pub format_count: usize,
    /// Has pending clipboard data.
    pub clipboard_pending: bool,
    /// Current drag session ID (0 = none).
    pub drag_session: u32,
    /// Statistics: messages sent.
    pub stats_sent: u64,
    /// Statistics: messages received.
    pub stats_received: u64,
}

impl VmGuest {
    /// Create empty guest.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            state: BridgeState::Disconnected,
            features: 0,
            last_activity: 0,
            sequence: 0,
            clipboard_formats: [0; 8],
            format_count: 0,
            clipboard_pending: false,
            drag_session: 0,
            stats_sent: 0,
            stats_received: 0,
        }
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.state == BridgeState::Connected
    }

    /// Check if feature supported.
    pub fn has_feature(&self, feature: u32) -> bool {
        (self.features & feature) != 0
    }

    /// Update formats.
    pub fn set_formats(&mut self, formats: &[FormatId]) {
        let count = formats.len().min(8);
        self.clipboard_formats[..count].copy_from_slice(&formats[..count]);
        self.format_count = count;
    }

    /// Check if format available.
    pub fn has_format(&self, format: FormatId) -> bool {
        self.clipboard_formats[..self.format_count].contains(&format)
    }

    /// Allocate next sequence number.
    pub fn next_sequence(&mut self) -> u32 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }
}

// ============================================================================
// Bridge Events
// ============================================================================

/// Bridge event.
#[derive(Clone, Copy, Debug)]
pub enum BridgeEvent {
    /// Guest connected.
    Connected { vm_id: u32, features: u32 },
    /// Guest disconnected.
    Disconnected { vm_id: u32 },
    /// Clipboard changed on guest.
    ClipboardChanged { vm_id: u32, formats: [FormatId; 8], count: usize },
    /// Drag started from guest.
    DragStarted { vm_id: u32, session_id: u32, formats: [FormatId; 8], count: usize },
    /// Drag dropped on host.
    DragDropped { vm_id: u32, session_id: u32, action: DragAction },
    /// File transfer started.
    FileTransferStarted { vm_id: u32, transfer_id: u32, filename: [u8; 64], size: u64 },
    /// File transfer completed.
    FileTransferCompleted { vm_id: u32, transfer_id: u32 },
    /// Error occurred.
    Error { vm_id: u32, code: u32 },
}

/// Event listener callback.
pub type BridgeEventFn = fn(event: BridgeEvent);

// ============================================================================
// VM Data Bridge
// ============================================================================

/// VM data bridge manager.
pub struct VmDataBridge {
    /// Connected guests.
    guests: [VmGuest; MAX_VM_GUESTS],
    /// Guest count.
    guest_count: usize,
    /// Pending transfers.
    transfers: [PendingTransfer; MAX_PENDING_TRANSFERS],
    /// Transfer count.
    transfer_count: usize,
    /// Event listener.
    event_listener: Option<BridgeEventFn>,
    /// Next transfer ID.
    next_transfer_id: u32,
    /// Current timestamp.
    timestamp: u64,
    /// Auto-sync clipboard.
    auto_sync: bool,
    /// Statistics: total syncs.
    stats_syncs: u64,
    /// Statistics: total transfers.
    stats_transfers: u64,
}

impl VmDataBridge {
    /// Create new bridge.
    pub const fn new() -> Self {
        Self {
            guests: [VmGuest::empty(); MAX_VM_GUESTS],
            guest_count: 0,
            transfers: [PendingTransfer::empty(); MAX_PENDING_TRANSFERS],
            transfer_count: 0,
            event_listener: None,
            next_transfer_id: 1,
            timestamp: 0,
            auto_sync: true,
            stats_syncs: 0,
            stats_transfers: 0,
        }
    }

    // ========================================================================
    // Guest Management
    // ========================================================================

    /// Register a new guest.
    pub fn register_guest(&mut self, vm_id: u32, features: u32) -> bool {
        if self.guest_count >= MAX_VM_GUESTS {
            return false;
        }

        // Check if already registered
        if self.find_guest(vm_id).is_some() {
            return false;
        }

        let guest = &mut self.guests[self.guest_count];
        *guest = VmGuest::empty();
        guest.id = vm_id;
        guest.features = features;
        guest.state = BridgeState::Connected;
        guest.last_activity = self.timestamp;
        self.guest_count += 1;

        // RAYOS_VMBRIDGE:CONNECTED
        self.emit_event(BridgeEvent::Connected { vm_id, features });

        true
    }

    /// Unregister a guest.
    pub fn unregister_guest(&mut self, vm_id: u32) -> bool {
        for i in 0..self.guest_count {
            if self.guests[i].id == vm_id {
                // Cancel pending transfers
                self.cancel_guest_transfers(vm_id);

                // Remove guest
                for j in i..self.guest_count - 1 {
                    self.guests[j] = self.guests[j + 1];
                }
                self.guests[self.guest_count - 1] = VmGuest::empty();
                self.guest_count -= 1;

                // RAYOS_VMBRIDGE:DISCONNECTED
                self.emit_event(BridgeEvent::Disconnected { vm_id });

                return true;
            }
        }
        false
    }

    /// Find guest by ID.
    pub fn find_guest(&self, vm_id: u32) -> Option<&VmGuest> {
        self.guests[..self.guest_count]
            .iter()
            .find(|g| g.id == vm_id)
    }

    /// Find guest by ID (mutable).
    pub fn find_guest_mut(&mut self, vm_id: u32) -> Option<&mut VmGuest> {
        self.guests[..self.guest_count]
            .iter_mut()
            .find(|g| g.id == vm_id)
    }

    /// Get connected guest count.
    pub fn connected_count(&self) -> usize {
        self.guests[..self.guest_count]
            .iter()
            .filter(|g| g.is_connected())
            .count()
    }

    // ========================================================================
    // Clipboard Operations
    // ========================================================================

    /// Sync clipboard to guest (host -> guest).
    pub fn sync_clipboard_to_guest(
        &mut self,
        vm_id: u32,
        selection: ClipboardSelection,
        entry: &ClipboardEntry,
    ) -> Result<u32, BridgeError> {
        let guest = self.find_guest_mut(vm_id).ok_or(BridgeError::GuestNotFound)?;

        if !guest.is_connected() {
            return Err(BridgeError::NotConnected);
        }

        if !guest.has_feature(VIRTIO_CLIPBOARD_F_PASTE) {
            return Err(BridgeError::FeatureNotSupported);
        }

        guest.state = BridgeState::Syncing;
        let sequence = guest.next_sequence();

        // Create transfer
        let transfer_id = self.create_transfer(vm_id, VirtioMsgType::SetData as u8, sequence)?;

        // Build message
        let header = VirtioMsgHeader::new(
            VirtioMsgType::SetData,
            selection,
            entry.formats[0].data.len() as u32,
        );

        // In real implementation, queue to virtio ring
        let _ = header.to_bytes();

        guest.state = BridgeState::Connected;
        guest.stats_sent += 1;
        self.stats_syncs += 1;
        // RAYOS_VMBRIDGE:SYNC_H2G

        Ok(transfer_id)
    }

    /// Request clipboard from guest (guest -> host).
    pub fn request_clipboard_from_guest(
        &mut self,
        vm_id: u32,
        selection: ClipboardSelection,
        format: FormatId,
    ) -> Result<u32, BridgeError> {
        let guest = self.find_guest_mut(vm_id).ok_or(BridgeError::GuestNotFound)?;

        if !guest.is_connected() {
            return Err(BridgeError::NotConnected);
        }

        if !guest.has_feature(VIRTIO_CLIPBOARD_F_COPY) {
            return Err(BridgeError::FeatureNotSupported);
        }

        if !guest.has_format(format) {
            return Err(BridgeError::FormatNotAvailable);
        }

        guest.state = BridgeState::Syncing;
        let sequence = guest.next_sequence();

        // Create transfer
        let transfer_id = self.create_transfer(vm_id, VirtioMsgType::GetData as u8, sequence)?;

        // Update transfer with format
        if let Some(transfer) = self.find_transfer_mut(transfer_id) {
            transfer.format = format;
        }

        // Build request
        let mut header = VirtioMsgHeader::new(VirtioMsgType::GetData, selection, 0);
        header.format = format as u16;
        header.sequence = sequence;

        // In real implementation, queue to virtio ring
        let _ = header.to_bytes();

        guest.state = BridgeState::Connected;
        guest.stats_sent += 1;
        // RAYOS_VMBRIDGE:SYNC_G2H

        Ok(transfer_id)
    }

    /// Notify guests of clipboard change.
    pub fn notify_clipboard_change(
        &mut self,
        selection: ClipboardSelection,
        formats: &[FormatId],
    ) {
        if !self.auto_sync {
            return;
        }

        for guest in &mut self.guests[..self.guest_count] {
            if guest.is_connected() && guest.has_feature(VIRTIO_CLIPBOARD_F_PASTE) {
                // Build format list message
                let mut header = VirtioMsgHeader::new(
                    VirtioMsgType::Formats,
                    selection,
                    (formats.len() * 4) as u32,
                );
                header.sequence = guest.next_sequence();

                // In real implementation, queue to virtio ring
                let _ = header.to_bytes();
                guest.stats_sent += 1;
            }
        }
    }

    // ========================================================================
    // Drag-Drop Operations
    // ========================================================================

    /// Start drag from guest.
    pub fn guest_drag_start(
        &mut self,
        vm_id: u32,
        formats: &[FormatId],
    ) -> Result<u32, BridgeError> {
        let guest = self.find_guest_mut(vm_id).ok_or(BridgeError::GuestNotFound)?;

        if !guest.is_connected() {
            return Err(BridgeError::NotConnected);
        }

        if !guest.has_feature(VIRTIO_CLIPBOARD_F_DRAGDROP) {
            return Err(BridgeError::FeatureNotSupported);
        }

        // Allocate session ID
        let session_id = self.next_transfer_id;
        self.next_transfer_id += 1;

        guest.drag_session = session_id;
        guest.set_formats(formats);

        let mut format_array = [0u32; 8];
        let count = formats.len().min(8);
        for i in 0..count {
            format_array[i] = formats[i];
        }

        self.emit_event(BridgeEvent::DragStarted {
            vm_id,
            session_id,
            formats: format_array,
            count,
        });

        Ok(session_id)
    }

    /// Notify guest of drop.
    pub fn guest_drop(
        &mut self,
        vm_id: u32,
        session_id: u32,
        action: DragAction,
    ) -> Result<(), BridgeError> {
        let guest = self.find_guest_mut(vm_id).ok_or(BridgeError::GuestNotFound)?;

        if guest.drag_session != session_id {
            return Err(BridgeError::InvalidSession);
        }

        // Build drop message
        let mut header = VirtioMsgHeader::new(
            VirtioMsgType::DragDrop,
            ClipboardSelection::Clipboard,
            1,
        );
        header.flags = action as u32;
        header.sequence = guest.next_sequence();

        // In real implementation, queue to virtio ring
        let _ = header.to_bytes();

        guest.drag_session = 0;
        guest.stats_sent += 1;

        self.emit_event(BridgeEvent::DragDropped {
            vm_id,
            session_id,
            action,
        });

        Ok(())
    }

    /// Cancel guest drag.
    pub fn guest_drag_cancel(&mut self, vm_id: u32) -> Result<(), BridgeError> {
        let guest = self.find_guest_mut(vm_id).ok_or(BridgeError::GuestNotFound)?;

        if guest.drag_session == 0 {
            return Ok(()); // No active drag
        }

        let header = VirtioMsgHeader::new(
            VirtioMsgType::DragCancel,
            ClipboardSelection::Clipboard,
            0,
        );

        // In real implementation, queue to virtio ring
        let _ = header.to_bytes();

        guest.drag_session = 0;
        guest.stats_sent += 1;

        Ok(())
    }

    // ========================================================================
    // Transfer Management
    // ========================================================================

    /// Create a pending transfer.
    fn create_transfer(
        &mut self,
        vm_id: u32,
        msg_type: u8,
        sequence: u32,
    ) -> Result<u32, BridgeError> {
        if self.transfer_count >= MAX_PENDING_TRANSFERS {
            // Try to cleanup expired transfers
            self.cleanup_transfers();
            if self.transfer_count >= MAX_PENDING_TRANSFERS {
                return Err(BridgeError::TooManyTransfers);
            }
        }

        let id = self.next_transfer_id;
        self.next_transfer_id += 1;

        self.transfers[self.transfer_count] = PendingTransfer {
            id,
            sequence,
            vm_id,
            state: TransferState::Pending,
            msg_type,
            format: 0,
            bytes_transferred: 0,
            total_bytes: 0,
            start_time: self.timestamp,
            timeout: 1000,
        };
        self.transfer_count += 1;

        Ok(id)
    }

    /// Find transfer by ID.
    pub fn find_transfer(&self, id: u32) -> Option<&PendingTransfer> {
        self.transfers[..self.transfer_count]
            .iter()
            .find(|t| t.id == id)
    }

    /// Find transfer by ID (mutable).
    fn find_transfer_mut(&mut self, id: u32) -> Option<&mut PendingTransfer> {
        self.transfers[..self.transfer_count]
            .iter_mut()
            .find(|t| t.id == id)
    }

    /// Complete a transfer.
    pub fn complete_transfer(&mut self, id: u32, success: bool) {
        if let Some(transfer) = self.find_transfer_mut(id) {
            transfer.state = if success {
                TransferState::Completed
            } else {
                TransferState::Failed
            };
            self.stats_transfers += 1;
            // RAYOS_VMBRIDGE:TRANSFER
        }
    }

    /// Cancel transfers for a guest.
    fn cancel_guest_transfers(&mut self, vm_id: u32) {
        for transfer in &mut self.transfers[..self.transfer_count] {
            if transfer.vm_id == vm_id && transfer.is_active() {
                transfer.state = TransferState::Failed;
            }
        }
    }

    /// Cleanup completed/expired transfers.
    fn cleanup_transfers(&mut self) {
        let mut i = 0;
        while i < self.transfer_count {
            let expired = self.transfers[i].is_timed_out(self.timestamp);
            let done = matches!(
                self.transfers[i].state,
                TransferState::Completed | TransferState::Failed | TransferState::Idle
            );

            if expired || done {
                for j in i..self.transfer_count - 1 {
                    self.transfers[j] = self.transfers[j + 1];
                }
                self.transfers[self.transfer_count - 1] = PendingTransfer::empty();
                self.transfer_count -= 1;
            } else {
                i += 1;
            }
        }
    }

    // ========================================================================
    // Message Handling
    // ========================================================================

    /// Handle incoming message from guest.
    pub fn handle_message(&mut self, vm_id: u32, header: &VirtioMsgHeader, _data: &[u8]) {
        let guest = match self.find_guest_mut(vm_id) {
            Some(g) => g,
            None => return,
        };

        guest.last_activity = self.timestamp;
        guest.stats_received += 1;

        match header.get_type() {
            VirtioMsgType::Formats => {
                // Guest is offering formats
                // Parse format list from data
                // For now, just mark as pending
                guest.clipboard_pending = true;

                self.emit_event(BridgeEvent::ClipboardChanged {
                    vm_id,
                    formats: guest.clipboard_formats,
                    count: guest.format_count,
                });
            }
            VirtioMsgType::Data => {
                // Guest sent clipboard data
                // Find matching transfer
                if let Some(transfer) = self.transfers[..self.transfer_count]
                    .iter_mut()
                    .find(|t| t.vm_id == vm_id && t.sequence == header.sequence)
                {
                    transfer.state = TransferState::Completed;
                    transfer.bytes_transferred = header.length as usize;
                    self.stats_syncs += 1;
                }
            }
            VirtioMsgType::DragStart => {
                // Guest started drag
                // Parse formats from data
                let session_id = self.next_transfer_id;
                self.next_transfer_id += 1;
                guest.drag_session = session_id;

                self.emit_event(BridgeEvent::DragStarted {
                    vm_id,
                    session_id,
                    formats: guest.clipboard_formats,
                    count: guest.format_count,
                });
            }
            VirtioMsgType::DragCancel => {
                let session_id = guest.drag_session;
                guest.drag_session = 0;
                // Notify host drag system
                let _ = session_id;
            }
            VirtioMsgType::FileStart => {
                // File transfer starting
                let transfer_id = self.next_transfer_id;
                self.next_transfer_id += 1;

                self.emit_event(BridgeEvent::FileTransferStarted {
                    vm_id,
                    transfer_id,
                    filename: [0u8; 64], // Would parse from data
                    size: header.length as u64,
                });
            }
            VirtioMsgType::FileComplete => {
                // File transfer done
                self.stats_transfers += 1;
            }
            VirtioMsgType::Error => {
                // Guest reported error
                self.emit_event(BridgeEvent::Error {
                    vm_id,
                    code: header.flags,
                });
            }
            _ => {}
        }
    }

    // ========================================================================
    // Event System
    // ========================================================================

    /// Set event listener.
    pub fn set_event_listener(&mut self, listener: BridgeEventFn) {
        self.event_listener = Some(listener);
    }

    /// Clear event listener.
    pub fn clear_event_listener(&mut self) {
        self.event_listener = None;
    }

    /// Emit event.
    fn emit_event(&self, event: BridgeEvent) {
        if let Some(listener) = self.event_listener {
            listener(event);
        }
    }

    // ========================================================================
    // Utilities
    // ========================================================================

    /// Tick the bridge (update timestamp, cleanup).
    pub fn tick(&mut self) {
        self.timestamp += 1;

        // Check for timed out transfers
        for transfer in &mut self.transfers[..self.transfer_count] {
            if transfer.is_active() && transfer.is_timed_out(self.timestamp) {
                transfer.state = TransferState::Failed;
                self.emit_event(BridgeEvent::Error {
                    vm_id: transfer.vm_id,
                    code: 1, // Timeout
                });
            }
        }

        // Periodic cleanup
        if self.timestamp % 100 == 0 {
            self.cleanup_transfers();
        }
    }

    /// Enable/disable auto-sync.
    pub fn set_auto_sync(&mut self, enabled: bool) {
        self.auto_sync = enabled;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64, usize) {
        (self.stats_syncs, self.stats_transfers, self.connected_count())
    }
}

// ============================================================================
// Bridge Errors
// ============================================================================

/// Bridge operation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    /// Guest not found.
    GuestNotFound,
    /// Not connected.
    NotConnected,
    /// Feature not supported by guest.
    FeatureNotSupported,
    /// Format not available.
    FormatNotAvailable,
    /// Invalid session.
    InvalidSession,
    /// Too many pending transfers.
    TooManyTransfers,
    /// Transfer failed.
    TransferFailed,
    /// Timeout.
    Timeout,
    /// Internal error.
    Internal,
}

// ============================================================================
// Global VM Data Bridge
// ============================================================================

/// Global VM data bridge.
static mut GLOBAL_BRIDGE: VmDataBridge = VmDataBridge::new();

/// Get VM data bridge.
pub fn vm_bridge() -> &'static VmDataBridge {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_BRIDGE }
}

/// Get VM data bridge (mutable).
pub fn vm_bridge_mut() -> &'static mut VmDataBridge {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_BRIDGE }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtio_header() {
        let header = VirtioMsgHeader::new(
            VirtioMsgType::GetFormats,
            ClipboardSelection::Clipboard,
            100,
        );

        let bytes = header.to_bytes();
        let parsed = VirtioMsgHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.get_type(), VirtioMsgType::GetFormats);
        assert_eq!(parsed.length, 100);
    }

    #[test]
    fn test_vm_guest() {
        let mut guest = VmGuest::empty();
        guest.id = 1;
        guest.features = VIRTIO_CLIPBOARD_F_COPY | VIRTIO_CLIPBOARD_F_PASTE;
        guest.state = BridgeState::Connected;

        assert!(guest.is_connected());
        assert!(guest.has_feature(VIRTIO_CLIPBOARD_F_COPY));
        assert!(!guest.has_feature(VIRTIO_CLIPBOARD_F_FILES));
    }

    #[test]
    fn test_pending_transfer() {
        let mut transfer = PendingTransfer::empty();
        transfer.id = 1;
        transfer.state = TransferState::Pending;
        transfer.start_time = 0;
        transfer.timeout = 100;

        assert!(transfer.is_active());
        assert!(!transfer.is_timed_out(50));
        assert!(transfer.is_timed_out(150));
    }

    #[test]
    fn test_bridge_guest_management() {
        let mut bridge = VmDataBridge::new();

        assert!(bridge.register_guest(1, VIRTIO_CLIPBOARD_F_COPY));
        assert_eq!(bridge.connected_count(), 1);

        assert!(bridge.find_guest(1).is_some());
        assert!(bridge.find_guest(2).is_none());

        assert!(bridge.unregister_guest(1));
        assert_eq!(bridge.connected_count(), 0);
    }

    #[test]
    fn test_transfer_progress() {
        let mut transfer = PendingTransfer::empty();
        transfer.total_bytes = 100;
        transfer.bytes_transferred = 50;

        assert_eq!(transfer.progress(), 50);
    }

    #[test]
    fn test_msg_type_from() {
        assert_eq!(VirtioMsgType::from(1), VirtioMsgType::GetFormats);
        assert_eq!(VirtioMsgType::from(10), VirtioMsgType::DragStart);
        assert_eq!(VirtioMsgType::from(99), VirtioMsgType::Error);
    }
}
