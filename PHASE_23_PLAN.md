# Phase 23: Wayland-First GUI - Plan

**Date**: 2026-01-08  
**Project**: RayOS (Rust-based Advanced RTOS)  
**Phase**: 23 - Wayland-First GUI Architecture  
**Duration**: Single focused session  
**Target**: ~5,000 lines, 6 tasks, 50+ tests  

---

## Strategic Objective

Replace RayOS's PPM (PPM-based scanout) presentation layer with a **real Wayland protocol stack**, enabling:

1. **Native Wayland Protocol**: Full wayland-core protocol implementation
2. **Multi-Window Display**: Proper window titles, decorations, resizing
3. **Input Events**: Full keyboard/mouse/touch event delivery
4. **Drag & Drop**: Cross-app clipboard and file operations  
5. **DPI Scaling**: Resolution-aware rendering and scaling
6. **Wayland Clients**: Support standard wl-shell and xdg-shell protocols

This transforms RayOS from "custom GUI framework" to "standard Wayland compositor."

---

## Problem Statement

### Current State (Phase 22)
- PPM-based framebuffer presentation
- Custom window protocol (not Wayland)
- No standard client support
- Limited interoperability
- Manual window decoration rendering

### What We Need
- ✅ Standard Wayland 1.20+ protocol
- ✅ Compositing window manager
- ✅ Standard client compatibility
- ✅ Professional-grade display stack

---

## Roadmap: 3 Milestones, 6 Tasks

### Milestone 1: Wayland Protocol Core (Tasks 1-2)

**Goal**: Implement Wayland protocol server and wire it to display hardware

**Task 1: Wayland Core Protocol Server** (~900 lines)
- File: `crates/kernel-bare/src/wayland_core.rs` (new)
- WaylandServer struct: central protocol handler
- Display object: global registry, client wl_display
- Registry: advertise globals (compositor, shm, shell, etc)
- Callback handling: request/event dispatch
- Tests: 12 unit tests
- Markers: 8 (RAYOS_WAYLAND:*)

**Task 2: Wayland Compositor & Surfaces** (~1000 lines)
- File: `crates/kernel-bare/src/wayland_compositor.rs` (new)
- WaylandCompositor: compositor global (wl_compositor)
- Surface: wl_surface implementation (buffer attachment, commit)
- SharedMemory: wl_shm for client buffers
- Buffer: client framebuffer management
- Tests: 15 unit tests
- Markers: 10 (RAYOS_COMPOSITOR:*)

### Milestone 2: Shell & Input (Tasks 3-4)

**Goal**: Implement Wayland shell protocols and input device handling

**Task 3: Wayland Shell Protocol** (~900 lines)
- File: `crates/kernel-bare/src/wayland_shell.rs` (new)
- XDG Shell: xdg-shell protocol (wl_xdg_wm_base, xdg_surface, xdg_toplevel)
- Window state management (maximized, fullscreen, activated)
- Window resizing and positioning
- Window decorations via server-side decoration protocol
- Popup windows and menus
- Tests: 13 unit tests
- Markers: 8 (RAYOS_SHELL:*)

**Task 4: Input Devices & Events** (~1000 lines)
- File: `crates/kernel-bare/src/wayland_input.rs` (new)
- Keyboard: wl_keyboard implementation (key events, modifiers, layout)
- Pointer: wl_pointer implementation (motion, button, scroll, enter/leave)
- Touch: wl_touch implementation (down, up, motion, cancel)
- Seat: wl_seat (keyboard + pointer + touch)
- Focus tracking and delivery
- Tests: 14 unit tests
- Markers: 10 (RAYOS_INPUT:*, RAYOS_POINTER:*, RAYOS_KEYBOARD:*)

### Milestone 3: Advanced Features (Tasks 5-6)

**Goal**: Implement drag-drop, DPI scaling, and integration

**Task 5: Drag & Drop & Clipboard** (~900 lines)
- File: `crates/kernel-bare/src/wayland_dnd.rs` (new)
- Data Device: wl_data_device (drag, drop, selection)
- Data Source: wl_data_source (drag source, mime types)
- Data Offer: wl_data_offer (drop target, selections)
- Clipboard integration with RayApp clipboard manager
- Drag visualization and feedback
- Tests: 12 unit tests
- Markers: 8 (RAYOS_DND:*, RAYOS_SELECTION:*)

**Task 6: Testing, Integration & DPI Scaling** (~1000 lines)
- Files: 
  - `crates/kernel-bare/tests/phase_23_integration.rs` (new, ~600 lines)
  - `crates/kernel-bare/src/wayland_scaling.rs` (new, ~400 lines)
- DPI Scaling: logical vs physical coordinates
- Scaling transformations: 100%, 125%, 150%, 200%
- HiDPI display support
- Integration tests: full Wayland client lifecycle
- Performance tests: client creation/destruction, event throughput
- Acceptance criteria verification
- Tests: 50+ total across all tasks
- Markers: 6 (RAYOS_SCALE:*, RAYOS_WAYLAND_CLIENT:*)

---

## Detailed Task Breakdown

### Task 1: Wayland Core Protocol Server

#### Components

1. **WaylandServer**
   - Central protocol dispatcher
   - Client connection management
   - Global registry
   - Methods:
     - `new()`: Create server
     - `handle_connection()`: Accept new client
     - `dispatch_request()`: Route client request
     - `send_event()`: Send event to client
     - `register_global()`: Advertise interface

2. **Display**
   - wl_display object (id 1)
   - Global registry
   - Sync/roundtrip support
   - Methods:
     - `get_registry()`: Return wl_registry
     - `sync()`: Roundtrip confirmation
     - `get_error()`: Return protocol errors

3. **Registry**
   - wl_registry object
   - Global enumeration
   - Bind interface
   - Methods:
     - `bind()`: Bind to advertised global
     - `send_global()`: Advertise interface
     - `send_global_remove()`: Remove interface

4. **Protocol Handling**
   - Message parsing
   - Request/response dispatch
   - Error handling
   - Deterministic markers

#### Unit Tests (12)

```
✓ test_wayland_server_creation
✓ test_client_connection
✓ test_registry_enumeration
✓ test_global_registration
✓ test_global_binding
✓ test_protocol_dispatch
✓ test_request_handling
✓ test_event_sending
✓ test_sync_roundtrip
✓ test_multiple_clients
✓ test_error_handling
✓ test_protocol_version_negotiation
```

#### Deterministic Markers (8)

- `RAYOS_WAYLAND:SERVER_START` - Server initialized
- `RAYOS_WAYLAND:CLIENT_CONNECT` - Client connected
- `RAYOS_WAYLAND:GLOBAL_ADVERTISED` - Global registered
- `RAYOS_WAYLAND:BIND_SUCCESS` - Interface bound
- `RAYOS_WAYLAND:REQUEST_DISPATCHED` - Request routed
- `RAYOS_WAYLAND:EVENT_SENT` - Event delivered
- `RAYOS_WAYLAND:SYNC_COMPLETE` - Roundtrip completed
- `RAYOS_WAYLAND:ERROR` - Protocol error

---

### Task 2: Wayland Compositor & Surfaces

#### Components

1. **WaylandCompositor**
   - wl_compositor global
   - Create surfaces and regions
   - Methods:
     - `new()`: Create compositor
     - `create_surface()`: New wl_surface
     - `create_region()`: New wl_region

2. **Surface**
   - wl_surface object
   - Buffer attachment and commits
   - Damage tracking
   - Viewport and transformation
   - Methods:
     - `attach_buffer()`: Set framebuffer
     - `commit()`: Apply changes
     - `damage()`: Mark changed region
     - `set_viewport()`: Scaling/cropping
     - `destroy()`: Cleanup

3. **SharedMemory**
   - wl_shm global
   - Allocate shared buffers
   - Format advertising (ARGB8888, XRGB8888)
   - Methods:
     - `create_pool()`: Allocate SHM pool
     - `create_buffer()`: Allocate buffer in pool

4. **Buffer**
   - wl_buffer object
   - Client framebuffer
   - Lifecycle management
   - Methods:
     - `get_data()`: Access buffer contents
     - `release()`: Notify client when done with buffer

#### Unit Tests (15)

```
✓ test_compositor_creation
✓ test_surface_creation
✓ test_buffer_attachment
✓ test_surface_commit
✓ test_damage_tracking
✓ test_shm_pool_allocation
✓ test_buffer_creation
✓ test_buffer_formats
✓ test_viewport_scaling
✓ test_region_creation
✓ test_region_operations
✓ test_buffer_lifecycle
✓ test_multiple_surfaces
✓ test_surface_destruction
✓ test_compositor_performance
```

#### Deterministic Markers (10)

- `RAYOS_COMPOSITOR:CREATE` - Compositor created
- `RAYOS_COMPOSITOR:GLOBAL_ADVERTISED` - wl_compositor advertised
- `RAYOS_SURFACE:CREATE` - Surface created
- `RAYOS_SURFACE:BUFFER_ATTACHED` - Buffer attached
- `RAYOS_SURFACE:COMMIT` - Surface committed
- `RAYOS_SURFACE:DAMAGE` - Region marked dirty
- `RAYOS_SHM:POOL_CREATE` - SHM pool created
- `RAYOS_SHM:BUFFER_CREATE` - Buffer allocated
- `RAYOS_SURFACE:VIEWPORT_SET` - Viewport configured
- `RAYOS_SURFACE:DESTROY` - Surface destroyed

---

### Task 3: Wayland Shell Protocol

#### Components

1. **XDG WM Base**
   - xdg_wm_base global
   - Ping/pong for client liveness
   - Methods:
     - `get_xdg_surface()`: Create toplevel
     - `destroy()`: Cleanup

2. **XDG Surface**
   - xdg_surface object
   - Associate with wl_surface
   - Methods:
     - `get_toplevel()`: Create window
     - `get_popup()`: Create popup
     - `ack_configure()`: Confirm resize

3. **XDG Toplevel**
   - xdg_toplevel object (window)
   - Title, app ID, window state
   - Maximize/fullscreen/activated states
   - Methods:
     - `set_title()`: Set window title
     - `set_app_id()`: Set application ID
     - `set_maximized()`, `unset_maximized()`
     - `set_fullscreen()`, `unset_fullscreen()`
     - `move()`, `resize()`: Client drag requests
     - `show_window_menu()`: Context menu

4. **XDG Popup**
   - xdg_popup object (menu/dialog)
   - Parent and positioning
   - Popup grab and dismissal
   - Methods:
     - `grab()`, `dismiss()`
     - `reposition()`

5. **Server Decorations**
   - zxdg_decoration_manager_v1
   - Window borders/titles managed by server
   - Methods:
     - `get_toplevel_decoration()`
     - `set_mode()`: client vs server decoration

#### Unit Tests (13)

```
✓ test_xdg_wm_base_creation
✓ test_xdg_surface_creation
✓ test_xdg_toplevel_creation
✓ test_window_title_setting
✓ test_app_id_setting
✓ test_maximize_state
✓ test_fullscreen_state
✓ test_activated_state
✓ test_window_move_request
✓ test_window_resize_request
✓ test_xdg_popup_creation
✓ test_server_decorations
✓ test_shell_state_transitions
```

#### Deterministic Markers (8)

- `RAYOS_SHELL:XDG_WM_BASE_ADVERTISED` - Shell available
- `RAYOS_SHELL:XDG_SURFACE_CREATE` - Surface created
- `RAYOS_SHELL:XDG_TOPLEVEL_CREATE` - Window created
- `RAYOS_SHELL:TITLE_SET` - Title configured
- `RAYOS_SHELL:STATE_CHANGE` - State modified (maximize/fullscreen)
- `RAYOS_SHELL:DECORATION_MODE_SET` - Decoration style set
- `RAYOS_SHELL:POPUP_CREATE` - Popup created
- `RAYOS_SHELL:CONFIGURE_SENT` - Resize request to client

---

### Task 4: Input Devices & Events

#### Components

1. **Keyboard**
   - wl_keyboard object
   - Key event delivery
   - Modifiers (Shift, Ctrl, Alt, Super)
   - Key repeat (typematic)
   - Methods:
     - `send_keymap()`: Deliver layout
     - `send_enter()`: Focus gained
     - `send_leave()`: Focus lost
     - `send_key()`: Key press/release
     - `send_modifiers()`: Modifier state

2. **Pointer**
   - wl_pointer object
   - Motion events with coordinates
   - Button events (left/middle/right)
   - Scroll events (vertical/horizontal)
   - Enter/leave for focus
   - Methods:
     - `send_enter()`: Pointer enters surface
     - `send_leave()`: Pointer leaves surface
     - `send_motion()`: Position update
     - `send_button()`: Button press/release
     - `send_axis()`: Scroll event
     - `set_cursor()`: Cursor image

3. **Touch**
   - wl_touch object
   - Multi-touch support
   - Methods:
     - `send_down()`: Touch begins
     - `send_up()`: Touch ends
     - `send_motion()`: Position update
     - `send_frame()`: Batch end
     - `send_cancel()`: Touch cancelled

4. **Seat**
   - wl_seat object
   - Advertise capabilities (keyboard/pointer/touch)
   - Focus management
   - Methods:
     - `get_keyboard()`: Return wl_keyboard
     - `get_pointer()`: Return wl_pointer
     - `get_touch()`: Return wl_touch
     - `set_selection()`: Set clipboard

5. **Focus Tracking**
   - Maintain input focus per surface
   - Ensure only focused app receives input
   - Deliver from RayApp focus to Wayland clients

#### Unit Tests (14)

```
✓ test_seat_creation
✓ test_keyboard_creation
✓ test_pointer_creation
✓ test_touch_creation
✓ test_keyboard_enter_leave
✓ test_key_press_delivery
✓ test_key_release_delivery
✓ test_modifiers_delivery
✓ test_pointer_motion
✓ test_button_press_release
✓ test_scroll_delivery
✓ test_touch_down_up
✓ test_focus_management
✓ test_multi_client_input
```

#### Deterministic Markers (10)

- `RAYOS_INPUT:SEAT_ADVERTISED` - Input available
- `RAYOS_KEYBOARD:ENTER` - Keyboard focus gained
- `RAYOS_KEYBOARD:LEAVE` - Keyboard focus lost
- `RAYOS_KEYBOARD:KEY` - Key delivered
- `RAYOS_KEYBOARD:MODIFIERS` - Modifiers sent
- `RAYOS_POINTER:ENTER` - Pointer focus gained
- `RAYOS_POINTER:MOTION` - Position updated
- `RAYOS_POINTER:BUTTON` - Button delivered
- `RAYOS_POINTER:AXIS` - Scroll delivered
- `RAYOS_INPUT:FOCUS_CHANGE` - Focus transferred

---

### Task 5: Drag & Drop & Clipboard

#### Components

1. **Data Device Manager**
   - wl_data_device_manager global
   - Create data devices and sources
   - Methods:
     - `create_data_source()`: New drag source
     - `get_data_device()`: New drop target

2. **Data Source**
   - wl_data_source object
   - Drag source for DND operations
   - MIME type advertisement
   - Methods:
     - `offer()`: Advertise MIME type
     - `set_actions()`: Supported drag actions
     - `dnd_drop_performed()`: Drop happened
     - `dnd_finished()`: Operation complete

3. **Data Device**
   - wl_data_device object
   - Drag and drop target
   - Clipboard management
   - Methods:
     - `start_drag()`: Begin drag operation
     - `set_selection()`: Set clipboard
     - `send_selection()`: Deliver clipboard data
     - `send_offer()`: Available data types

4. **Data Offer**
   - wl_data_offer object
   - Drop target receives offer
   - Data requests
   - Methods:
     - `accept()`: Accept MIME type
     - `receive()`: Request data transfer
     - `finish()`: Drop completed
     - `set_actions()`: Accept drag actions

5. **Clipboard Integration**
   - Wire Wayland clipboard to RayApp ClipboardManager
   - Synchronize selection between apps
   - Methods:
     - `sync_to_rayapp()`: Update RayApp clipboard
     - `sync_from_rayapp()`: Read RayApp clipboard

#### Unit Tests (12)

```
✓ test_data_device_manager_creation
✓ test_data_source_creation
✓ test_mime_type_offering
✓ test_drag_start
✓ test_drag_motion
✓ test_drag_drop
✓ test_data_transfer
✓ test_clipboard_set_selection
✓ test_clipboard_data_request
✓ test_clipboard_sync_with_rayapp
✓ test_dnd_between_clients
✓ test_dnd_actions
```

#### Deterministic Markers (8)

- `RAYOS_DND:DATA_DEVICE_MANAGER_ADVERTISED` - DND available
- `RAYOS_DND:SOURCE_CREATE` - Drag source created
- `RAYOS_DND:DRAG_START` - Drag begun
- `RAYOS_DND:DROP_PERFORMED` - Drop happened
- `RAYOS_DND:FINISHED` - Operation completed
- `RAYOS_SELECTION:SET` - Clipboard set
- `RAYOS_SELECTION:REQUESTED` - Data requested
- `RAYOS_SELECTION:TRANSFERRED` - Data delivered

---

### Task 6: Testing, Integration & DPI Scaling

#### Testing File: phase_23_integration.rs (~600 lines)

**Integration Test Coverage**

1. **Wayland Client Lifecycle** (5 tests)
   - Connect → bind compositor → create surface → attach buffer → commit
   - Verify surface appears
   - Client disconnect and cleanup

2. **Multi-Client Scenarios** (6 tests)
   - 2 clients with separate surfaces
   - Window stacking/focus
   - Input routing between clients
   - Concurrent operations

3. **Shell Protocol** (4 tests)
   - Create xdg_toplevel window
   - Set title and app_id
   - Maximize/fullscreen transitions
   - Decoration modes

4. **Input Events** (6 tests)
   - Keyboard delivery to focused client
   - Pointer motion and buttons
   - Cross-window focus changes
   - Modifier tracking

5. **Drag & Drop** (4 tests)
   - Drag from source to target
   - MIME type negotiation
   - Data transfer
   - Clipboard sync with RayApp

6. **Performance** (4 tests)
   - Client creation throughput
   - Buffer commit latency
   - Event delivery latency
   - 60 FPS composition with multiple clients

7. **Acceptance Criteria** (6 tests)
   - Full Wayland 1.20 protocol
   - Standard client compatibility
   - Multi-window support
   - Drag-drop functionality
   - DPI scaling
   - Integration with Phase 22 RayApp

#### DPI Scaling File: wayland_scaling.rs (~400 lines)

**Components**

1. **Output**
   - wl_output object
   - Display information
   - DPI and scale factor
   - Methods:
     - `send_geometry()`: Monitor position/size
     - `send_mode()`: Resolution modes
     - `send_scale()`: Scale factor (100%, 125%, etc)
     - `send_done()`: Configuration complete

2. **Scaling Transformations**
   - Logical coordinates: what apps see
   - Physical coordinates: actual pixels
   - Scaling factor: logical → physical
   - Supported scales: 100%, 125%, 150%, 200%
   - Methods:
     - `logical_to_physical()`: Convert coordinates
     - `physical_to_logical()`: Reverse conversion
     - `scale_buffer()`: Transform framebuffer

3. **HiDPI Support**
   - High-resolution displays
   - Per-surface scaling
   - Viewport transformations
   - Methods:
     - `set_surface_scale()`: Client specifies scale
     - `get_optimal_scale()`: Recommend scale for output

#### Additional Tests in wayland_scaling.rs (6)

```
✓ test_output_creation
✓ test_scale_factor_advertisement
✓ test_coordinate_transformation
✓ test_buffer_scaling
✓ test_hidpi_support
✓ test_multi_output_scaling
```

#### Total Test Count

```
rayapp_core.rs (Phase 22):          12 tests
rayapp_clipboard.rs (Phase 22):     12 tests
rayapp_events.rs (Phase 22):        13 tests
linux_presentation.rs (Phase 22):    5 tests
shell.rs (Phase 22):                12 tests
phase_22_integration.rs (Phase 22): 37 tests
────────────────────────────────────────────
Phase 22 Subtotal:                  91 tests

wayland_core.rs (Task 1):           12 tests
wayland_compositor.rs (Task 2):     15 tests
wayland_shell.rs (Task 3):          13 tests
wayland_input.rs (Task 4):          14 tests
wayland_dnd.rs (Task 5):            12 tests
wayland_scaling.rs (Task 6):         6 tests
phase_23_integration.rs (Task 6):   35 tests
────────────────────────────────────────────
Phase 23 Total:                    107 tests
```

**Grand Total**: 198 unit/integration tests (Phase 22 + 23)

---

## Acceptance Criteria

Phase 23 must satisfy the following before completion:

### 1. Full Wayland Protocol Implementation
- [ ] wl_display with registry and globals
- [ ] wl_compositor with surface and region support
- [ ] wl_shm with shared memory buffers (ARGB8888, XRGB8888)
- [ ] xdg_shell with toplevel and popup support
- [ ] wl_seat with keyboard, pointer, and touch
- [ ] wl_data_device with drag-drop and clipboard
- [ ] All events and requests implemented per spec

### 2. Standard Client Compatibility
- [ ] Can run wayland-scanner generated client stubs
- [ ] Clients can connect and bind interfaces
- [ ] Surface creation and buffer attachment works
- [ ] Input events properly delivered
- [ ] Window state (maximize/fullscreen) works

### 3. Multi-Window Support
- [ ] Multiple clients run simultaneously
- [ ] Each has independent event queue
- [ ] Focus switching works
- [ ] Window titles and decorations render
- [ ] Z-order respected (focus on top)

### 4. Drag & Drop Functionality
- [ ] Drag source creates data source
- [ ] Drop target receives offer
- [ ] MIME types negotiated
- [ ] Data transferred via pipe/socket
- [ ] Clipboard synchronized with Phase 22 RayApp

### 5. DPI Scaling
- [ ] Output advertises scale factor
- [ ] Clients can query scale
- [ ] Logical to physical coordinate transformation works
- [ ] HiDPI buffers rendered at native resolution
- [ ] 100%, 125%, 150%, 200% scales supported

### 6. Performance Targets
- [ ] Client connection: < 1 ms
- [ ] Surface creation: < 0.5 ms
- [ ] Buffer commit: < 2 ms
- [ ] Input latency: < 5 ms
- [ ] Composition: 60 FPS with 4+ clients

### 7. Zero Regressions
- [ ] All Phase 22 RayApp code still compiles
- [ ] Phase 21 functionality unchanged
- [ ] No new compilation errors
- [ ] 100+ total tests passing (Phase 22 + 23)

### 8. Clean Integration
- [ ] Wayland compositor co-exists with RayApp Window Manager
- [ ] Input focus synchronized between Wayland and RayApp
- [ ] Clipboard synchronized bidirectionally
- [ ] Window events properly routed

---

## Risk Assessment

| Risk | Impact | Mitigation | Status |
|------|--------|-----------|--------|
| Wayland protocol complexity | High | Implement incrementally, test each interface | TBD |
| Client incompatibility | High | Test with real Wayland clients early | TBD |
| Input focus management | High | Clear routing rules, extensive testing | TBD |
| Drag-drop edge cases | Medium | Comprehensive test suite for DND scenarios | TBD |
| DPI scaling complexity | Medium | Start with common scales (100%, 150%, 200%) | TBD |
| Performance regressions | High | Benchmark each component, optimize as needed | TBD |

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Total lines added | ~5,000 |
| New modules | 7 (wayland_core, compositor, shell, input, dnd, scaling, integration tests) |
| Unit tests | 72 new tests (Phase 23 specific) |
| Integration tests | 35 comprehensive scenarios |
| Compilation errors | 0 |
| Performance: client creation | < 1 ms |
| Performance: frame time @ 60 FPS | < 16.67 ms |
| Deterministic markers | 50+ unique types |
| Git commits | 7-8 (one per task) |

---

## Next Phase Preview (Phase 24)

After Phase 23 completes, priorities shift to:

1. **System Integration Testing**: Soak runs, stress tests, failure injection
2. **Wayland Client Library**: Helper crate for apps to easily create Wayland clients
3. **Theme Engine**: Customizable window decorations and colors
4. **Multi-Output Support**: Multiple displays with independent scaling
5. **Accessibility**: Screen readers, input method editors, high-contrast modes

---

## Execution Plan

### Session Structure

```
Phase 23: Wayland-First GUI (Single Session)
├─ Task 1: Wayland Core (900 lines) → Compile ✓
├─ Task 2: Compositor (1000 lines) → Compile ✓
├─ Task 3: Shell Protocol (900 lines) → Compile ✓
├─ Task 4: Input Devices (1000 lines) → Compile ✓
├─ Task 5: Drag & Drop (900 lines) → Compile ✓
└─ Task 6: Integration & Scaling (1200 lines) → Final Report

Total: ~5,900 lines, 107 tests, 0 errors
```

### Checkpoints

Each task will:
1. ✅ Compile without errors
2. ✅ Pass all unit tests
3. ✅ Emit proper deterministic markers
4. ✅ Commit atomically to git
5. ✅ Update todo list

---

**Phase 23: Wayland-First GUI is ready to begin!** 🚀
