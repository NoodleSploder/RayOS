# Phase 30: Drag-and-Drop & Clipboard Integration

**Phase Goal**: Implement cross-app drag-and-drop, clipboard sharing, and file picker dialogs for seamless data transfer
**Target Lines**: 3,500+ (700 per task)
**Target Tests**: 68+ (13-14 per task)
**Target Markers**: 25 (5 per task)
**Target Errors**: 0
**Status**: PLANNING

---

## Phase 30 Overview

Building on Phase 29's window management and RayApp runtime, Phase 30 delivers the data transfer primitives needed for a productive desktop experience. This includes drag-and-drop between apps/desktop, clipboard operations, and file picker dialogs.

### Architecture Integration
```
Phase 30 (Drag-and-Drop & Clipboard)
         ↓
Phase 29 (Window Manager & RayApp Runtime)
         ↓
Phase 28 (Networking & Content Delivery)
         ↓
Phase 27 (Audio & Accessibility)
         ↓
Phases 1-26 (Core Infrastructure)
```

### Design Alignment

From RAYOS_TODO.md - RayOS-native GUI requirements:
- Clipboard integration between RayOS apps and VM guests
- Drag-and-drop for file transfer and data sharing
- File picker dialogs for controlled filesystem access

From App Runtime (Phase 29):
- AppIPC provides messaging foundation for clipboard/drag operations
- CapabilitySet enforces clipboard access permissions
- AppSandbox controls what apps can read/write

---

## Task 1: Clipboard Manager

**Objective**: Unified clipboard with format negotiation and VM guest bridge
**File**: `clipboard.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_CLIPBOARD:*)

### Components
- `ClipboardFormat`: Text, RichText, Html, Image, FilePath, Custom(u32)
- `ClipboardEntry`: Format + data + timestamp + owner_app_id
- `ClipboardHistory`: Ring buffer for last 10 clipboard entries
- `ClipboardSelection`: Primary (middle-click) vs Clipboard (Ctrl+C/V)
- `ClipboardManager`: Set, get, clear, watch, history access
- `ClipboardBridge`: Virtio-clipboard for VM guest sharing
- `ClipboardEvent`: Copied, Pasted, Cleared, FormatChanged
- `ClipboardPolicy`: Per-app clipboard access control
- Tests: Copy/paste text, format conversion, history, VM bridge

### Key Features
- Multiple formats per entry (text + html + image)
- Lazy format conversion on request
- VM clipboard sync via virtio protocol
- History with per-entry metadata (app, timestamp)
- Policy enforcement (which apps can read/write)

---

## Task 2: Drag-and-Drop Engine

**Objective**: Cross-app drag-and-drop with visual feedback and format negotiation
**File**: `drag_drop.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_DND:*)

### Components
- `DragSource`: AppId + WindowId + supported formats + data
- `DragPayload`: Format + inline data or deferred callback
- `DragVisual`: Cursor overlay (icon + label + effect indicator)
- `DropTarget`: WindowId + accepted formats + drop zone bounds
- `DropEffect`: None, Copy, Move, Link
- `DragSession`: Active drag state with source, target candidates, position
- `DragDropManager`: Begin, update, end, cancel operations
- `HitTestResult`: Target window + zone + accepted formats
- Tests: Drag within app, drag between apps, drop on desktop, cancel

### Key Features
- Visual drag feedback (custom cursor/icon)
- Format negotiation (source offers, target accepts)
- Copy vs Move semantics
- Drop zone highlighting
- Cancel on Escape or leaving valid targets

---

## Task 3: File Picker Dialog

**Objective**: Native file picker for sandboxed file access
**File**: `file_picker.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_PICKER:*)

### Components
- `PickerMode`: Open, Save, OpenMultiple, SelectFolder
- `FileFilter`: Extension list, MIME type, custom predicate
- `PickerState`: Navigating, Selected, Confirmed, Cancelled
- `DirectoryEntry`: Name, size, modified, is_dir, icon
- `DirectoryView`: List view with sorting, selection, navigation
- `FilePicker`: Modal dialog with path bar, file list, filter dropdown
- `PickerResult`: Selected path(s) or cancellation
- `PickerPolicy`: Restrict accessible directories per app
- Tests: Open file, save file, multi-select, folder select, filter

### Key Features
- Sandboxed access (app only sees selected files)
- Recent files list
- Favorites/bookmarks sidebar
- Path breadcrumb navigation
- Preview pane for images/text

---

## Task 4: Data Transfer Formats

**Objective**: Format registry and conversion engine for clipboard/DnD
**File**: `data_transfer.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_TRANSFER:*)

### Components
- `FormatId`: Unique format identifier (well-known + custom)
- `FormatRegistry`: Register, lookup, list formats
- `FormatConverter`: Convert between compatible formats
- `WellKnownFormats`: TEXT, UTF8, HTML, PNG, JPEG, FILE_LIST, URI_LIST
- `DataBlob`: Opaque data with format tag and size
- `DataProvider`: Deferred data generation callback
- `ConversionPath`: Chain of converters for A → B
- `ConversionResult`: Success(data) or NotSupported
- Tests: Format registration, conversion chains, deferred data

### Key Features
- Extensible format system
- Automatic format conversion where possible
- Deferred data (don't copy until needed)
- Format priority for negotiation
- Binary-safe data handling

---

## Task 5: VM Guest Data Bridge

**Objective**: Clipboard and drag-drop bridge to Linux/Windows VMs
**File**: `vm_data_bridge.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_VMDATA:*)

### Components
- `VirtioClipboard`: Device model for guest clipboard sync
- `VirtioClipboardEvent`: SetContents, GetContents, FormatList
- `GuestDragSource`: Drag initiated from guest surface
- `GuestDropTarget`: Drop accepted by guest surface
- `BridgeProtocol`: Request/response format for guest agent
- `FormatTranslation`: Map RayOS formats ↔ guest formats
- `BridgeState`: Connected, Syncing, Idle, Error
- `VmDataBridgeManager`: Per-VM bridge lifecycle
- Tests: Host→guest copy, guest→host paste, cross-VM transfer

### Key Features
- Bidirectional clipboard sync
- Format translation (handle Linux/Windows differences)
- Drag from host to guest and vice versa
- Rate limiting to prevent clipboard storms
- Graceful degradation if guest agent unavailable

---

## Success Criteria

- [ ] All 5 tasks implement assigned components
- [ ] 3,500+ lines of code
- [ ] 68+ unit + 25+ scenario tests (93+ total)
- [ ] 25 custom markers (RAYOS_CLIPBOARD, RAYOS_DND, etc.)
- [ ] 0 compilation errors
- [ ] Full no-std compliance
- [ ] Integration with Phase 29 app runtime
- [ ] Clean git history (atomic commits per task)

---

## Timeline

- **Task 1** (Clipboard Manager): ~20 min → compile → commit
- **Task 2** (Drag-Drop Engine): ~20 min → compile → commit
- **Task 3** (File Picker Dialog): ~25 min → compile → commit
- **Task 4** (Data Transfer Formats): ~20 min → compile → commit
- **Task 5** (VM Guest Data Bridge): ~25 min → compile → commit
- **Final Report**: ~10 min → commit
- **Total**: ~120 minutes

---

## Integration Points

### With Phase 29 (Window Manager & RayApp)
- `DragDropManager` uses `InputRouter` for mouse tracking
- `FilePicker` creates a modal window via `WindowManager`
- `ClipboardPolicy` uses `AppSandbox` capability checks
- `ClipboardEvent` routes through `AppIPC`

### With Phase 28 (Networking)
- Remote clipboard sync (future)
- URL drag-drop creates network fetch

### With Hypervisor/VMM
- `VirtioClipboard` device model for guest sync
- `GuestDragSource/Target` for VM surface drag-drop
- Format translation for Linux/Windows compatibility

### Future Phases
- App Store integration (Phase 31)
- Cloud sync for clipboard history (Phase 32)
- Accessibility for drag-drop (announce actions)

---

## Notes

- All components use no-std, fixed-size arrays
- Clipboard data capped at 16MB per entry
- Drag visual uses existing renderer primitives
- File picker reuses existing widget framework
- VM bridge uses existing virtio infrastructure

---

## File Locations

All new files go in `crates/kernel-bare/src/ui/`:

| Task | File |
|------|------|
| 1 | `clipboard.rs` |
| 2 | `drag_drop.rs` |
| 3 | `file_picker.rs` |
| 4 | `data_transfer.rs` |
| 5 | `vm_data_bridge.rs` |

Update `mod.rs` to include new modules after each task.

---

## Markers Reference

| Task | Markers |
|------|---------|
| 1 | RAYOS_CLIPBOARD:COPIED, RAYOS_CLIPBOARD:PASTED, RAYOS_CLIPBOARD:CLEARED, RAYOS_CLIPBOARD:SYNCED, RAYOS_CLIPBOARD:FORMAT |
| 2 | RAYOS_DND:STARTED, RAYOS_DND:UPDATED, RAYOS_DND:DROPPED, RAYOS_DND:CANCELLED, RAYOS_DND:EFFECT |
| 3 | RAYOS_PICKER:OPENED, RAYOS_PICKER:NAVIGATED, RAYOS_PICKER:SELECTED, RAYOS_PICKER:CONFIRMED, RAYOS_PICKER:CANCELLED |
| 4 | RAYOS_TRANSFER:REGISTERED, RAYOS_TRANSFER:CONVERTED, RAYOS_TRANSFER:DEFERRED, RAYOS_TRANSFER:PROVIDED, RAYOS_TRANSFER:FAILED |
| 5 | RAYOS_VMDATA:CONNECTED, RAYOS_VMDATA:HOST_TO_GUEST, RAYOS_VMDATA:GUEST_TO_HOST, RAYOS_VMDATA:TRANSLATED, RAYOS_VMDATA:ERROR |

---

*This plan continues the RayOS desktop experience with essential data transfer primitives.*
