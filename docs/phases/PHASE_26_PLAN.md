# Phase 26 Plan: Display Server Integration
## Bringing Graphics Pipeline to Life with Wayland Protocol

**Status:** 🚀 **PLANNED**
**Start Date:** January 8, 2026
**Target Completion:** Single intensive session

---

## Overview

Phase 26 transitions from graphics infrastructure (Phase 25) to a fully functional display server by implementing Wayland protocol support, input handling, window management, and display backend drivers. This phase builds directly on the advanced rendering pipeline created in Phase 25.

**Phase 26 Goals:**
- Implement core Wayland protocol (wl_surface, wl_buffer, wl_output)
- Create input event handling system (keyboard, mouse, touch)
- Develop window management with focus/stacking
- Integrate display backend drivers (framebuffer output)
- Achieve functional Wayland display server for single-monitor setup

---

## Task Breakdown (5 tasks, ~3,500 lines target)

### Task 1: Wayland Protocol Core (900 lines)
**File:** `wayland_protocol.rs`
**Target:** 900 lines | Tests: 16 unit + 5 scenario | Markers: 5 (RAYOS_WAYLAND:*)

**Components:**
- `WaylandProtocolVersion`: Protocol version negotiation (1.0-1.23)
- `WaylandMessage`: Message types (request, event, error)
- `WaylandInterface`: Interface registry (wl_display, wl_registry, wl_callback)
- `WaylandObject`: Global object tracking (surfaces, outputs, seats)
- `RegistryManager`: Dynamic global discovery
- `SurfaceRole`: Role assignment (shell_surface, subsurface, cursor)
- `BufferManager`: wl_buffer lifecycle (attach, commit, release)
- `OutputInfo`: Display output description (resolution, refresh rate, make/model)
- `OutputMode`: Video mode definition (width, height, refresh in mHz)
- `WaylandServer`: Core protocol dispatcher

**Key Features:**
- Protocol message marshaling/unmarshaling
- Global interface advertisement
- Object ID management
- Event queue for dispatching
- Error handling and recovery
- Version negotiation

**Tests:**
- Unit: Message encoding, interface registry, buffer tracking, output modes, object IDs
- Scenario: Client connection sequence, global advertisement, interface binding, buffer lifecycle

**Markers:**
- RAYOS_WAYLAND:CONNECT - Client connection
- RAYOS_WAYLAND:INTERFACE - Interface binding
- RAYOS_WAYLAND:SURFACE - Surface creation
- RAYOS_WAYLAND:BUFFER - Buffer attachment
- RAYOS_WAYLAND:ERROR - Protocol error

---

### Task 2: Input Event System (850 lines)
**File:** `input_events.rs`
**Target:** 850 lines | Tests: 15 unit + 5 scenario | Markers: 5 (RAYOS_INPUT:*)

**Components:**
- `KeyboardEvent`: Key press/release data
  - Key code (Linux keymap)
  - Key state (pressed/released)
  - Modifiers (Shift, Ctrl, Alt, Super)
  - Timestamp (milliseconds)
- `PointerEvent`: Mouse/touchpad movement
  - Position (x, y surface-relative)
  - Delta (dx, dy for relative motion)
  - Button state (left/middle/right)
  - Axis (scroll wheel, horizontal scroll)
- `TouchEvent`: Touchscreen input
  - Touch ID (finger tracking)
  - Position (x, y)
  - Pressure (0.0-1.0)
  - Contact shape (major/minor axis)
- `InputDevice`: Hardware device abstraction
  - Device ID, device name
  - Capabilities (keyboard/pointer/touch)
  - Repeat rate for keyboard
- `KeyboardFocus`: Focus management
  - Focused surface ID
  - Key state array (256 keys)
  - Current modifier state
- `PointerFocus`: Pointer tracking
  - Position, button state
  - Hovered surface
  - Cursor theme/size
- `EventDispatcher`: Route events to focused client
  - Input device tracking
  - Event queue (up to 512 events)
  - Seat abstraction (wl_seat)
- `HitTest`: Layer-aware event routing
  - Ray-casting into composited layers
  - Surface containment checking
  - Z-order respecting

**Key Features:**
- Multi-device support (keyboard, mouse, touch)
- Event replay and debugging
- Double-click detection (300ms window)
- Key repeat support
- Seat abstraction matching Wayland spec
- Hit-testing against composited surfaces

**Tests:**
- Unit: Key events, pointer motion, touch input, focus management, modifier keys
- Scenario: Keyboard focus switch, pointer enter/leave, touch gesture start/update/end, multi-device

**Markers:**
- RAYOS_INPUT:DEVICE - Input device discovery
- RAYOS_INPUT:KEYBOARD - Keyboard event
- RAYOS_INPUT:POINTER - Pointer event
- RAYOS_INPUT:TOUCH - Touch event
- RAYOS_INPUT:FOCUS - Focus change

---

### Task 3: Window Management (800 lines)
**File:** `window_management.rs`
**Target:** 800 lines | Tests: 14 unit + 5 scenario | Markers: 5 (RAYOS_WINDOW:*)

**Components:**
- `Window`: Single window abstraction
  - Window ID, title, role
  - Position (x, y), dimensions (width, height)
  - State (normal, minimized, maximized, fullscreen)
  - Z-order, active status
  - Parent window (modal dialogs)
- `WindowRole`: Role types
  - TopLevel (main application windows)
  - Dialog (modal/non-modal)
  - Popup (menus, tooltips)
  - Notification
- `WindowManager`: Window collection and ordering
  - Window list (up to 256 windows)
  - Focus stack (most recent first)
  - Stacking order enforcement
  - Z-order management
- `LayoutMode`: Tiling algorithm
  - Floating (freeform positioning)
  - Tile (automatic tiling)
  - Tabbed (tab-based grouping)
  - Monocle (fullscreen-like)
- `TilingLayout`: Automatic window arrangement
  - Master-stack layout (1 master, N stack)
  - Main window ratio (adjustable)
  - Master count (adjustable)
  - Gap size (workspace padding)
- `Workspace`: Virtual desktop
  - Window set per workspace
  - Active workspace tracking
  - Workspace switching
- `WindowLifecycle`: State machine
  - Map (create & show)
  - Focus
  - Minimize/maximize/fullscreen
  - Unmap (destroy)
- `FocusPolicy`: Focus behavior
  - Click-to-focus
  - Follow-mouse
  - Sloppy-focus

**Key Features:**
- Multi-window support with Z-ordering
- Tiling layout engine (master-stack)
- Modal dialog handling
- Window state transitions
- Keyboard focus/pointer focus independence
- Workspace support for virtual desktops
- Automatic layout recalculation

**Tests:**
- Unit: Window creation, stacking, focus, layout calculations, state transitions
- Scenario: Tiling algorithm, workspace switching, modal dialog, focus follow, minimize/maximize

**Markers:**
- RAYOS_WINDOW:CREATE - Window creation
- RAYOS_WINDOW:LAYOUT - Layout calculation
- RAYOS_WINDOW:FOCUS - Focus change
- RAYOS_WINDOW:STACK - Z-order change
- RAYOS_WINDOW:DESTROY - Window destruction

---

### Task 4: Display Backend Drivers (800 lines)
**File:** `display_drivers.rs`
**Target:** 800 lines | Tests: 14 unit + 5 scenario | Markers: 5 (RAYOS_DISPLAY:*)

**Components:**
- `DisplayMode`: Video mode definition
  - Width, height (pixels)
  - Refresh rate (mHz)
  - Aspect ratio
  - Flags (interlaced, preferred)
- `DisplayConnector`: Physical display connection
  - Connector ID (HDMI-1, eDP-1, etc.)
  - Connector type (HDMI, DisplayPort, eDP)
  - Connected status
  - EDID data (manufacturer, model, serial)
  - Available modes list
- `DisplayController`: Display hardware abstraction
  - Framebuffer pointer (physical address)
  - Pitch (bytes per line)
  - Bpp (bytes per pixel: 2, 3, 4)
  - Current mode
  - Gamma LUT support
- `FramebufferLayout`: Framebuffer configuration
  - Primary framebuffer (main display)
  - Secondary framebuffer (extended display - future)
  - Size and stride
  - Pixel format (RGB565, RGB888, XRGB8888, ARGB8888)
- `EdidParser`: EDID parsing
  - Manufacturer ID (3-letter code)
  - Product code and serial
  - Resolution and timing info
  - Color space, gamma
  - Preferred mode extraction
- `PixelFormat`: Framebuffer format
  - RGB565 (16-bit, 5:6:5)
  - RGB888 (24-bit)
  - XRGB8888 (32-bit, X padding)
  - ARGB8888 (32-bit with alpha)
- `VSyncManager`: Vertical sync control
  - Enable/disable vsync
  - Frame pacing
  - Frame callback timing (wl_frame_callback)
- `CrtcManager`: CRTC (cathode ray tube controller) management
  - Physical display connector assignment
  - Mode switching
  - Scanout framebuffer selection
- `DrmBridge`: Direct Rendering Manager bridge
  - IOCTL interface (minimal)
  - Property querying
  - Framebuffer configuration

**Key Features:**
- EDID parsing for display capabilities
- Multi-mode support (resolution switching)
- Pixel format abstraction (8/16/32-bit)
- Vsync-based frame pacing
- CRTC assignment to connectors
- Framebuffer flipping/double-buffering
- Resolution auto-detection

**Tests:**
- Unit: Display mode parsing, EDID parsing, pixel format conversion, CRTC assignment
- Scenario: Mode switching, multi-connector (future), EDID extraction, framebuffer flipping

**Markers:**
- RAYOS_DISPLAY:DETECT - Display detection
- RAYOS_DISPLAY:MODE - Mode selection
- RAYOS_DISPLAY:EDID - EDID parsing
- RAYOS_DISPLAY:FLIP - Framebuffer flip
- RAYOS_DISPLAY:VSYNC - Vsync callback

---

### Task 5: Server Integration & Event Loop (800 lines)
**File:** `display_server.rs`
**Target:** 800 lines | Tests: 13 unit + 5 scenario | Markers: 5 (RAYOS_SERVER:*)

**Components:**
- `ServerState`: Global server state
  - Wayland protocol instance
  - Input system
  - Window manager
  - Display drivers
  - Client list
  - Current time
- `ClientConnection`: Per-client state
  - Client ID
  - File descriptor (for socket communication - simulated)
  - Resource map (surfaces, buffers, etc.)
  - Event queue
  - Access token (for security)
- `ServerConfig`: Configuration
  - Display mode (resolution, refresh)
  - Input repeat rate
  - Workspace count
  - Layout mode (tiling/floating)
  - Keybindings
- `EventLoop`: Main server loop
  - Event queue processing
  - Input event dispatch
  - Surface commit handling
  - Frame callback triggering
  - Dirty region detection
  - Compositing trigger
- `SurfaceCommit`: Surface state update
  - Buffer attachment
  - Damage region
  - Transform (rotation)
  - Scale factor
  - Viewport changes
- `FrameCallback`: Frame completion signal
  - Callback data
  - Frame time (milliseconds)
  - One-shot fire mechanism
- `ServerMetrics`: Metrics tracking
  - Frame time, FPS
  - Event processing time
  - Client count
  - Surface count
- `ErrorHandler`: Error recovery
  - Client disconnect on protocol error
  - Graceful degradation
  - Error logging
- `SurfaceRenderer`: Integration point
  - Connects Wayland surfaces to compositing pipeline
  - Buffer to GPU memory mapping
  - Damage tracking integration
  - Frame callback integration

**Key Features:**
- Event-driven main loop
- Frame-synchronized rendering
- Client isolation and error recovery
- Per-frame metrics collection
- Integration with compositing pipeline
- Damage-aware rendering
- Buffer pool management

**Tests:**
- Unit: Server state, client management, frame timing, event dispatch
- Scenario: Complete frame sequence, client connect/disconnect, surface commit, frame callback

**Markers:**
- RAYOS_SERVER:INIT - Server initialization
- RAYOS_SERVER:CLIENT - Client connection
- RAYOS_SERVER:FRAME - Frame processing
- RAYOS_SERVER:DISPATCH - Event dispatch
- RAYOS_SERVER:RENDER - Render submission

---

## Success Criteria

### Code Quality
- ✅ 3,500+ lines of Wayland implementation
- ✅ 68+ tests (unit + scenario)
- ✅ 25 markers for CI/CD
- ✅ 0 compilation errors
- ✅ Full no-std compatibility

### Functionality
- ✅ Basic Wayland protocol support
- ✅ Input event handling (keyboard, mouse, touch)
- ✅ Window management with focus
- ✅ Display mode negotiation
- ✅ Frame callback timing
- ✅ Damage region optimization

### Integration
- ✅ Compositing pipeline integration
- ✅ Graphics API from Phase 25 usage
- ✅ GPU memory from Phase 25
- ✅ HDR/color from Phase 25
- ✅ Optimization profiling from Phase 25

### Documentation
- ✅ Comprehensive Phase 26 Final Report
- ✅ Component inventory
- ✅ Architecture decisions
- ✅ Roadmap to Phase 27

---

## Roadmap to Phase 27

Phase 27 should address:

1. **Extended Wayland Features**
   - xdg-shell protocol (standard shell interface)
   - wl_output scaling/transform
   - wl_data_device (copy-paste)
   - wl_data_device_manager (drag-drop)

2. **Advanced Input**
   - Relative pointer (FPS games)
   - Tablet input
   - Joystick/gamepad support

3. **Multi-Monitor Support**
   - Multiple displays
   - Display layout (mirror, extend, clone)
   - Hot-plug detection

4. **Performance Optimization**
   - Buffer pooling
   - Async commits
   - Presentation timing

5. **Security & Isolation**
   - Client process isolation
   - Resource limits per client
   - Capability-based security model

---

## Git Strategy

- Task 1 (Wayland Protocol): Commit with "Phase 26 Task 1: ..." message
- Task 2 (Input Events): Commit with "Phase 26 Task 2: ..." message
- Task 3 (Window Management): Commit with "Phase 26 Task 3: ..." message
- Task 4 (Display Drivers): Commit with "Phase 26 Task 4: ..." message
- Task 5 (Server Integration): Commit with "Phase 26 Task 5: ..." message
- Final Report: Commit with "Phase 26 Final Report: ..." message

**Total Commits:** 6 (5 tasks + 1 final report)

---

## Notes

- Phase 26 depends entirely on Phase 25 graphics pipeline
- Single-monitor setup as MVP (multi-monitor deferred to Phase 27)
- Wayland socket simulation (actual socket I/O deferred to Phase 27)
- Focus on core protocol and event handling
- Integration with compositing pipeline critical for rendering

---

**Phase 26 Status:** 🚀 Ready to begin
**Est. Duration:** Single intensive session
**Target Start:** Immediately after Phase 25 completion

