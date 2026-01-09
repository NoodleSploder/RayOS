# Phase 29: Window Manager & RayApp Runtime

**Phase Goal**: Build advanced window management and RayApp runtime infrastructure for multi-window desktop experience
**Target Lines**: 3,500+ (700 per task)
**Target Tests**: 68+ (13-14 per task)
**Target Markers**: 25 (5 per task)
**Target Errors**: 0
**Status**: PLANNING

---

## Phase 29 Overview

Building on Phase 28's networking infrastructure and the existing UI framework (window_manager, compositor, app_sdk), Phase 29 delivers production-ready window management with multi-GuestSurface embedding, advanced input routing, and a complete RayApp runtime. This phase enables RayOS to host multiple concurrent applications including VM surfaces as native windows.

### Architecture Integration
```
Phase 29 (Window Manager & RayApp Runtime)
         ↓
Phase 28 (Networking & Content Delivery)
         ↓
Phase 27 (Audio & Accessibility)
         ↓
Phase 26 (Display Server)
         ↓
Phase 25 (Graphics Pipeline)
         ↓
Phases 1-24 (Kernel Core)
```

### Design Alignment

From RAYOS_TODO.md - RayOS-native GUI requirements:
- Window manager layer: embed multiple `GuestSurface`s as resizable/focusable windows
- Input routing: route RayOS pointer/keyboard events into selected window surface
- RayApp abstraction: lifecycle hooks, UI framework, embedded surfaces
- App launcher: commands to instantiate/manage RayApps

From SENTIENT_SUBSTRATE.md - Integration with Bicameral Kernel:
- System 1 reflexes handle fast UI patterns (gaze, gestures)
- System 2 provides intent resolution for app commands

---

## Task 1: Multi-Window Surface Manager

**Objective**: Manage multiple GuestSurface instances with z-order, focus, and lifecycle
**File**: `surface_manager.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_SURFACE:*)

### Components
- `SurfaceId`: Unique surface identifier type
- `SurfaceState`: NotReady, Ready, Presented, Hidden, Destroyed
- `SurfaceMetadata`: Size, format, frame sequence, backing info
- `GuestSurfaceEntry`: Surface + metadata + window binding
- `SurfaceRegistry`: Registry for 16 surfaces max with lifecycle tracking
- `SurfaceBinding`: Maps SurfaceId ↔ WindowId for compositor integration
- `SurfaceFrameBuffer`: Ring buffer for frame data (8 frames max per surface)
- `SurfaceMetrics`: Frame rate, latency, dropped frames per surface
- `SurfaceManager`: Main orchestration (register, present, hide, destroy)
- Tests: Surface lifecycle, frame buffering, z-order, binding, metrics

### Key Features
- O(1) surface lookup by ID
- Frame sequence tracking for dropped-frame detection
- Surface-to-window binding with automatic unbind on destroy
- Backpressure signaling when frame buffer is full
- Statistics per surface (fps, latency, drops)

---

## Task 2: Advanced Window Manager

**Objective**: Extend window manager with tiling, snap, maximize, and workspace support
**File**: `window_manager_ext.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_WINDOW:*)

### Components
- `WindowLayout`: Floating, TiledLeft, TiledRight, Maximized, Fullscreen
- `SnapZone`: Left, Right, Top, Bottom, TopLeft, TopRight, BottomLeft, BottomRight
- `WindowConstraints`: Min/max size, aspect ratio, resize grip zones
- `WindowAnimation`: Move, Resize, Fade, Minimize, Restore animation state
- `WorkspaceId`: Workspace identifier (4 workspaces max)
- `Workspace`: Window set per workspace with active tracking
- `WorkspaceManager`: Workspace switching, window assignment
- `WindowSnapper`: Edge detection and snap-to-zone logic
- `WindowTiler`: Automatic tiling layout engine
- Tests: Snap zones, tiling, maximize/restore, workspace switching

### Key Features
- Windows snap to screen edges and half/quarter zones
- Maximize preserves previous geometry for restore
- Animated transitions for snap/maximize (integrate with animation.rs)
- 4 virtual workspaces with independent z-order
- Window follows focus across workspace switch

---

## Task 3: Input Router & Focus Engine

**Objective**: Route keyboard/mouse events to focused window with capture and grab support
**File**: `input_router.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_INPUT:*)

### Components
- `InputTarget`: WindowId or SystemGlobal (for shortcuts)
- `FocusStack`: Focus history for Alt-Tab cycling (8 entries)
- `KeyboardGrab`: Exclusive keyboard capture (for VM surfaces, dialogs)
- `PointerGrab`: Exclusive pointer capture (for resize, drag)
- `InputFilter`: Predicate for routing decisions (window bounds, visibility)
- `ShortcutBinding`: Global hotkey → action mapping (32 bindings max)
- `InputEvent`: Unified event type (key, mouse, gesture)
- `InputRouter`: Main dispatcher with grab/filter/route logic
- `FocusPolicy`: ClickToFocus, FocusFollowsMouse, Explicit
- Tests: Focus cycling, grab semantics, shortcut dispatch, VM input routing

### Key Features
- Alt-Tab focus cycling with visual switcher support
- Global shortcuts (Super+D = show desktop, Super+L = lock)
- Keyboard/pointer grabs for modal interactions
- VM surface input injection (routed to virtio-input)
- Focus policy configurable per window type

---

## Task 4: RayApp Runtime

**Objective**: App lifecycle management, registry, and launcher with sandboxing hooks
**File**: `app_runtime.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_APP:*)

### Components
- `AppId`: Unique app instance identifier
- `AppState`: Loading, Running, Suspended, Terminated
- `AppInstance`: Descriptor + State + WindowId + Resources
- `AppRegistry`: Running apps registry (8 apps max)
- `AppLauncher`: Create app instance, allocate window, call on_init
- `AppScheduler`: Frame budget, tick distribution across apps
- `AppSandbox`: Capability enforcement (file, network, clipboard)
- `AppIPC`: Inter-app message channel (64 messages queued max)
- `AppLifecycleHooks`: Pre-launch, post-terminate, suspend/resume
- Tests: Launch, terminate, suspend/resume, IPC, capability checks

### Key Features
- Apps run in cooperative scheduling (frame budget per tick)
- Capability enforcement based on AppDescriptor
- IPC for clipboard and drag-drop operations
- Suspend/resume for background apps
- Launch from shell command (`run <app>`)

---

## Task 5: Shell Integration & App Launcher UI

**Objective**: Desktop shell with app launcher, taskbar, and notification area
**File**: `shell_integration.rs` (~700 lines)
**Tests**: 13-14 unit + 5 scenario
**Markers**: 5 (RAYOS_SHELL:*)

### Components
- `TaskbarEntry`: WindowId, icon, title, state indicator
- `Taskbar`: Horizontal bar with window buttons (16 entries max)
- `AppLauncherMenu`: Grid of installed apps with search
- `NotificationArea`: System tray icons, clock, status indicators
- `NotificationToast`: Popup notification with timeout
- `DesktopIcon`: Clickable icon on desktop background
- `ShellState`: Taskbar visible, launcher open, notification queue
- `ShellCommand`: Launch, Close, Minimize, SwitchWorkspace, Lock
- `ShellEventHandler`: Routes shell-level input (Super key, taskbar clicks)
- Tests: Taskbar updates, launcher open/close, notification display, commands

### Key Features
- Taskbar shows all windows with focus indication
- Click taskbar entry to focus/raise window
- Super key opens app launcher overlay
- Notification toasts with 5-second timeout
- Clock in notification area (from system time)

---

## Success Criteria

- [ ] All 5 tasks implement assigned components
- [ ] 3,500+ lines of code
- [ ] 68+ unit + 25+ scenario tests (93+ total)
- [ ] 25 custom markers (RAYOS_SURFACE, RAYOS_WINDOW, etc.)
- [ ] 0 compilation errors
- [ ] Full no-std compliance
- [ ] Integration with existing ui/ modules
- [ ] Clean git history (atomic commits per task)

---

## Timeline

- **Task 1** (Surface Manager): ~20 min → compile → commit
- **Task 2** (Window Manager Ext): ~20 min → compile → commit
- **Task 3** (Input Router): ~20 min → compile → commit
- **Task 4** (App Runtime): ~25 min → compile → commit
- **Task 5** (Shell Integration): ~25 min → compile → commit
- **Final Report**: ~10 min → commit
- **Total**: ~120 minutes

---

## Integration Points

### With Existing UI Framework
- `surface_manager.rs` integrates with `compositor.rs` for frame compositing
- `window_manager_ext.rs` extends `window_manager.rs` (same Window struct)
- `input_router.rs` replaces/wraps `input.rs` dispatch logic
- `app_runtime.rs` uses `app_sdk.rs` App trait and AppContext
- `shell_integration.rs` extends `shell.rs` with launcher/taskbar

### With Phase 28 (Networking)
- App capabilities can request NETWORK permission
- AppIPC can use HTTP/WebSocket for remote app communication
- Notification toasts can be triggered by network events

### With Hypervisor/VMM
- `SurfaceManager` binds GuestSurface from virtio-gpu to windows
- `InputRouter` injects keyboard/mouse to VM via virtio-input
- VM window focus triggers keyboard grab for exclusive input

### Future Phases
- Drag-and-drop between apps (Phase 30?)
- App Store integration (Phase 31?)
- Cloud sync for app state (Phase 32?)

---

## Notes

- All components use no-std, fixed-size arrays
- Animation integration uses existing `animation.rs` module
- Window decorations rendered by compositor (existing)
- VM surfaces use existing `guest_surface` infrastructure
- Shell commands integrate with existing prompt parser

---

## File Locations

All new files go in `crates/kernel-bare/src/ui/`:

| Task | File |
|------|------|
| 1 | `surface_manager.rs` |
| 2 | `window_manager_ext.rs` |
| 3 | `input_router.rs` |
| 4 | `app_runtime.rs` |
| 5 | `shell_integration.rs` |

Update `mod.rs` to include new modules after each task.

---

## Markers Reference

| Task | Markers |
|------|---------|
| 1 | RAYOS_SURFACE:REGISTERED, RAYOS_SURFACE:PRESENTED, RAYOS_SURFACE:HIDDEN, RAYOS_SURFACE:DESTROYED, RAYOS_SURFACE:FRAME |
| 2 | RAYOS_WINDOW:SNAPPED, RAYOS_WINDOW:MAXIMIZED, RAYOS_WINDOW:TILED, RAYOS_WINDOW:WORKSPACE, RAYOS_WINDOW:RESTORED |
| 3 | RAYOS_INPUT:FOCUSED, RAYOS_INPUT:GRABBED, RAYOS_INPUT:SHORTCUT, RAYOS_INPUT:ROUTED, RAYOS_INPUT:RELEASED |
| 4 | RAYOS_APP:LAUNCHED, RAYOS_APP:RUNNING, RAYOS_APP:SUSPENDED, RAYOS_APP:TERMINATED, RAYOS_APP:IPC |
| 5 | RAYOS_SHELL:TASKBAR, RAYOS_SHELL:LAUNCHER, RAYOS_SHELL:NOTIFY, RAYOS_SHELL:COMMAND, RAYOS_SHELL:DESKTOP |

---

*This plan aligns with RAYOS_TODO.md requirements for RayOS-native GUI and follows the established phase structure.*
