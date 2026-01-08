# Phase 22 Plan: RayApp Framework for Native Application Container

**Date**: January 8, 2026
**Status**: Planning
**Target Completion**: Single session
**Estimated Lines**: ~4,800
**Modules**: 6 new + 1 enhanced existing

---

## Executive Summary

Phase 21 created the foundation: native presentation, installer, boot management, and observability. Now Phase 22 builds the application platform that makes RayOS practical for end-users: **RayApp Framework**.

This phase transforms RayOS from a "system we can boot" into a "system we can develop applications for." We implement:

1. **Native application containers** (isolated runtimes for guest apps)
2. **Window management** (focus, Z-ordering, lifecycle)
3. **Clipboard & file sandboxing** (safe inter-app data exchange)
4. **Multi-app coordination** (event routing, IPC)
5. **Surface composition** (render multiple apps as windows)
6. **CLI app management** (launch, close, focus, list)

**Success criteria**: 50+ unit tests, 0 errors, 0 regressions, 40+ deterministic markers, ~4,800 lines.

---

## Current State Analysis

### What Exists (from Phase 21 + earlier)

- ✅ `linux_presentation.rs` (560 lines): PresentationBridge for native scanout
- ✅ `guest_surface.rs` (existing): GuestSurface struct for frame buffers
- ✅ `rayapp.rs` (278 lines): Partial RayApp infrastructure
  - RayAppEntry (name, state, surface, Z-index, focus)
  - RayAppService (allocation, release, state tracking)
  - Deterministic markers (RAYOS_GUI_APP_READY, RAYOS_GUI_SURFACE_FRAME)
- ✅ `shell.rs` (enhanced): Command dispatcher (ready for new commands)

### What's Missing

**Critical gaps in rayapp.rs**:
1. **No window decoration API**: Can't set titles, handle minimize/maximize
2. **No clipboard ops**: Apps can't copy/paste between each other
3. **No file I/O sandbox**: Apps can't request files from RayOS
4. **No IPC/event routing**: Apps can't communicate or receive input events
5. **No surface composition**: Can't render multiple window decorations
6. **No shell commands**: No way to launch/close apps from CLI

---

## Phase 22 Milestones

### Milestone 1: Window Abstraction (Task 1-2)
**Goal**: Make RayApp more than a surface container — add window semantics.

**Deliverables**:
- Window creation/destruction API with titles + properties
- Focus management (only one app receives input at a time)
- Z-ordering (visual layering for multi-app)
- Window decoration metadata (minimize, maximize, close)
- Unit tests for all window lifecycle states

**Acceptance criteria**:
- Can create/destroy windows by app name
- Focus routing works (input goes to active window only)
- Z-order changes are observable
- 15+ unit tests passing

---

### Milestone 2: Data Exchange & Sandboxing (Task 3)
**Goal**: Enable safe app-to-app and app-to-kernel data exchange.

**Deliverables**:
- Clipboard API (get/set shared buffer)
- File request API (apps ask RayOS for file access)
- Path validation + sandbox enforcement
- Permission checks (which apps can access which paths)
- Unit tests for access control

**Acceptance criteria**:
- Clipboard get/set works bidirectionally
- File requests are validated
- Denied requests don't crash kernel
- 15+ unit tests passing

---

### Milestone 3: Multi-App Coordination & Surface Rendering (Tasks 4-6)
**Goal**: Turn multiple surfaces into a cohesive GUI with window decorations.

**Deliverables**:
- Event routing (keyboard/mouse to correct window based on Z-order)
- Inter-app communication (message queue for window events)
- Surface composition (render all active surfaces + decorations)
- Shell commands for app lifecycle (launch, close, focus, list, clipboard)
- Comprehensive testing + CI markers

**Acceptance criteria**:
- Can launch multiple apps and switch between them
- Window decorations render correctly
- Shell commands work for all operations
- 50+ unit tests passing
- 0 pre-existing regressions

---

## Task Breakdown

### Task 1: Window Lifecycle & Focus Management (700 lines)

**Location**: [rayapp.rs](crates/kernel-bare/src/rayapp.rs) enhancement

**Modules to add/enhance**:

1. **WindowProperties struct**
   - title (up to 64 bytes)
   - window_state (Normal, Minimized, Maximized, Hidden)
   - flags (resizable, closeable, focusable)
   - preferred_size (width, height)

2. **RayAppEntry enhancements**
   - Add window_properties field
   - Add input_enabled flag
   - Add last_focus_time (for LRU window cycling)

3. **RayAppService methods**
   - `set_window_title(id, title)` → bool
   - `set_window_state(id, state)` → bool
   - `get_window_properties(id)` → Option<WindowProperties>
   - `next_focused_window()` → Option<u8> (for Tab switching)
   - `window_count()` → usize (for status display)

4. **Focus policy**
   - Only one app has `input_enabled = true`
   - Switching focus clears input_enabled on old app
   - When app closes, focus moves to topmost remaining app

**Deterministic markers**:
- `RAYOS_GUI_WINDOW:CREATE:name`
- `RAYOS_GUI_WINDOW:FOCUS:name`
- `RAYOS_GUI_WINDOW:STATE_CHANGE:name:state`
- `RAYOS_GUI_WINDOW:DESTROY:name`

**Tests**:
- Create/destroy windows (5 tests)
- Focus management & LRU (5 tests)
- Window state transitions (5 tests)
- Input routing (5 tests)

---

### Task 2: Clipboard & File Sandbox (600 lines)

**New file**: [rayapp_clipboard.rs](crates/kernel-bare/src/rayapp_clipboard.rs) (400 lines)

**Modules**:

1. **ClipboardBuffer**
   - Shared 16 KB buffer
   - Write by owner, read by others
   - CRC32 validation
   - Timestamp tracking

2. **ClipboardManager**
   - Thread-safe get/set (with spinlock)
   - Ownership tracking (which app owns current clipboard)
   - History (last 5 clipboard values)
   - Clear on app shutdown

3. **FileAccessRequest struct**
   - requested_path (e.g., "/rayos/documents/file.txt")
   - access_type (Read, Write, Delete)
   - requester_app_id
   - permissions mask

4. **FileAccessPolicy**
   - Define which apps can access which paths
   - /rayos/public/* → all apps (read-only)
   - /rayos/app/<appid>/* → only that app
   - /rayos/tmp/* → all apps (read-write)

5. **FileRequestAPI**
   - `request_file_handle(id, path, access)` → Result<u64, Error>
   - `validate_path(path)` → bool (runs sandbox checks)
   - `check_app_permission(id, path)` → bool

**Deterministic markers**:
- `RAYOS_GUI_CLIPBOARD:SET:size`
- `RAYOS_GUI_CLIPBOARD:GET:size`
- `RAYOS_GUI_FILEIO:REQUEST:app:path`
- `RAYOS_GUI_FILEIO:GRANT:handle`
- `RAYOS_GUI_FILEIO:DENY:reason`

**Tests**:
- Clipboard get/set (5 tests)
- Clipboard ownership (3 tests)
- File request validation (5 tests)
- Permission enforcement (5 tests)
- Sandbox escape prevention (3 tests)

---

### Task 3: Inter-App Communication & Event Routing (700 lines)

**New file**: [rayapp_events.rs](crates/kernel-bare/src/rayapp_events.rs) (400 lines)

**Modules**:

1. **Event types**
   - WindowFocusGained
   - WindowFocusLost
   - KeyboardEvent (key, modifiers, pressed)
   - MouseEvent (x, y, button, pressed)
   - ClipboardChanged (new owner)

2. **EventQueue**
   - Per-app 64-entry queue
   - Spinlock-protected
   - Overflow handling (drop oldest if full)

3. **EventRouter**
   - Route input events based on focus + Z-order
   - Forward clipboard change notifications
   - Handle focus lost/gained transitions

4. **InterAppMessage struct**
   - source_app_id
   - dest_app_id (or broadcast)
   - message_type (string, 16 bytes)
   - payload (64 bytes)

5. **MessageQueue**
   - Per-app queues
   - Broadcast support (send to all apps)
   - Drain on app shutdown

**Deterministic markers**:
- `RAYOS_GUI_IPC:SEND:from:to:type`
- `RAYOS_GUI_IPC:BROADCAST:type`
- `RAYOS_GUI_EVENT:KEYBOARD:key:pressed`
- `RAYOS_GUI_EVENT:MOUSE:x:y:button`

**Tests**:
- Event queue insertion/drainage (5 tests)
- Input routing by Z-order (5 tests)
- Inter-app messaging (5 tests)
- Broadcast notifications (3 tests)
- Event overflow handling (3 tests)

---

### Task 4: Surface Rendering & Composition (800 lines)

**Enhancement to**: [linux_presentation.rs](crates/kernel-bare/src/linux_presentation.rs)

**New modules**:

1. **WindowDecorationRenderer**
   - Draw title bar (text + close button)
   - Draw shadow/border (1px white border + 2px drop shadow)
   - Draw Z-order badges (optional, for debug)
   - Composite decorations onto framebuffer

2. **SurfaceCompositor**
   - Maintain ordered list of active surfaces (by Z-order)
   - Composite all surfaces + decorations into final framebuffer
   - Handle partial updates (only redraw changed regions)
   - Alpha blending for shadows

3. **ScanoutOptimizer**
   - Track dirty regions per surface
   - Minimize full framebuffer redraws
   - Cache decorator renders (title text is stable)
   - Estimate GPU memory usage

4. **DisplayLayout**
   - Define tiling strategy (fullscreen, 2x2 grid, etc.)
   - Per-layout surface position calculator
   - Aspect ratio preservation
   - Handle dynamic surface count changes

**Rendering strategy**:
- Z-order 0 (background) = full screen / tiled
- Z-order 1+ = overlays with decorations
- Decoration height = 24px (title bar)
- Border = 1px white, shadow = 2px gray

**Deterministic markers**:
- `RAYOS_GUI_RENDER:COMPOSITE:app_count`
- `RAYOS_GUI_RENDER:DIRTY_REGION:x:y:w:h`
- `RAYOS_GUI_RENDER:SCANOUT:frame_id`

**Tests**:
- Decoration rendering (5 tests)
- Surface ordering (5 tests)
- Dirty region tracking (5 tests)
- Composition correctness (5 tests)
- Overlay blending (3 tests)

---

### Task 5: App Lifecycle Shell Commands (400 lines)

**Enhancement to**: [shell.rs](crates/kernel-bare/src/shell.rs)

**Commands to add**:

1. **cmd_app_launch**
   ```
   app launch <name> [width] [height]
   ```
   - Allocate app entry
   - Emit ready marker
   - Success/failure response

2. **cmd_app_close**
   ```
   app close <name>
   ```
   - Release app entry
   - Trigger focus change if needed
   - Emit close marker

3. **cmd_app_focus**
   ```
   app focus <name>
   ```
   - Set active app
   - Emit focus marker
   - Return new focus window name

4. **cmd_app_list**
   ```
   app list
   ```
   - Show all running apps with state
   - Show Z-order and focus status
   - Show surface frame count

5. **cmd_clipboard_set**
   ```
   clipboard set <text>
   ```
   - Set clipboard content
   - Take ownership
   - Return size

6. **cmd_clipboard_get**
   ```
   clipboard get
   ```
   - Read clipboard content
   - Show owner app name
   - Return content

7. **cmd_app_status**
   ```
   app status
   ```
   - Show active app details
   - Show focused window
   - Show surface info

**Deterministic markers**:
- `RAYOS_GUI_CMD:LAUNCH:name:result`
- `RAYOS_GUI_CMD:CLOSE:name:result`
- `RAYOS_GUI_CMD:FOCUS:name:result`

**Tests**:
- Command parsing (5 tests)
- Launch/close flow (5 tests)
- Focus switching (3 tests)
- Clipboard operations (3 tests)

---

### Task 6: Testing & Integration (600 lines)

**Locations**: Throughout modules + new [tests/](tests/) directory

**Test categories**:

1. **Unit tests** (45 tests, ~250 lines)
   - Window lifecycle (5)
   - Clipboard operations (5)
   - File sandbox (5)
   - Event routing (5)
   - Surface composition (5)
   - Shell commands (5)
   - Focus management (5)
   - Clipboard ownership (5)

2. **Integration tests** (15 tests, ~200 lines)
   - Multi-app launch/close sequence
   - Focus switching under load
   - Clipboard handoff between apps
   - Event routing correctness
   - Deterministic marker emission

3. **Regression tests** (10 tests, ~100 lines)
   - Phase 21 functionality (presentation, boot, observability)
   - No breakage from Phase 22 changes

4. **Acceptance criteria**
   - All 70 tests pass
   - 0 panics
   - 0 deadlocks
   - No phase 21 regressions
   - All deterministic markers emitted correctly

**Deterministic markers for CI** (40+ total):
- `RAYOS_GUI_WINDOW:*` (8)
- `RAYOS_GUI_CLIPBOARD:*` (4)
- `RAYOS_GUI_FILEIO:*` (4)
- `RAYOS_GUI_IPC:*` (4)
- `RAYOS_GUI_EVENT:*` (4)
- `RAYOS_GUI_RENDER:*` (4)
- `RAYOS_GUI_CMD:*` (8)

**CI automation**:
- Run headless smoke test: launch app → check markers → close app
- Verify no new panics vs Phase 21
- Check deterministic marker counts

---

## Implementation Order

**Suggested sequence** (tasks build on each other):

1. **Task 1** (Window Lifecycle): Extend rayapp.rs with window properties + focus
2. **Task 2** (Clipboard & Sandbox): Add rayapp_clipboard.rs
3. **Task 3** (IPC & Events): Add rayapp_events.rs
4. **Task 4** (Rendering): Enhance linux_presentation.rs
5. **Task 5** (Shell Commands): Add commands to shell.rs
6. **Task 6** (Testing): Add tests + validate integration

**Compilation checkpoints**:
- After Task 1: `cargo check` should pass (rayapp.rs changes only)
- After Task 2: `cargo check` + new file compile
- After Task 3: `cargo check` + IPC plumbing works
- After Task 4: `cargo check` + rendering integration
- After Task 5: `cargo build` + shell commands work
- After Task 6: `cargo test` + all tests pass

---

## Metrics & Success Criteria

| Metric | Target | Status |
|--------|--------|--------|
| Total lines added | ~4,800 | 📋 Planned |
| New files | 2 (rayapp_clipboard.rs, rayapp_events.rs) | 📋 Planned |
| Enhanced files | 3 (rayapp.rs, linux_presentation.rs, shell.rs) | 📋 Planned |
| Unit tests | 45+ | 📋 Planned |
| Integration tests | 15+ | 📋 Planned |
| Regression tests | 10+ | 📋 Planned |
| Deterministic markers | 40+ | 📋 Planned |
| Compilation errors | 0 | ✅ Required |
| Pre-existing regressions | 0 | ✅ Required |
| Focus cycles (Tab) | Works | 🎯 Acceptance |
| Clipboard exchange | Works | 🎯 Acceptance |
| App launch/close | Works | 🎯 Acceptance |
| Surface composition | Works | 🎯 Acceptance |

---

## Dependencies & Prerequisites

**What Phase 22 depends on**:
- ✅ Phase 21 foundation (native presentation, boot, observability)
- ✅ Existing linux_presentation.rs (scanout plumbing)
- ✅ Existing guest_surface.rs (frame buffer structs)
- ✅ Existing rayapp.rs (basic allocation + entry tracking)
- ✅ Existing shell.rs (command dispatcher)

**What Phase 22 enables**:
- Phase 23: Wayland-first GUI (real Wayland surfaces instead of PPM)
- Phase 24: System Integration Testing (soak tests under load)
- Phase 25: Production Hardening (security + encryption)

---

## Documentation & Knowledge Transfer

**Phase 22 creates**:
1. PHASE_22_PLAN.md (this file)
2. PHASE_22_FINAL_REPORT.md (after completion)
3. Code comments in all new modules
4. Test documentation (test names explain intent)

**Phase 22 updates**:
1. [shell.rs](crates/kernel-bare/src/shell.rs) comments
2. [rayapp.rs](crates/kernel-bare/src/rayapp.rs) architecture doc
3. Main.rs (new module declarations)

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Spinlock deadlock in multi-app | Medium | High | Use Compare-and-swap spinlock + timeout detection |
| Memory exhaustion | Low | High | Cap surface count to 4 apps, validate allocations |
| Event queue overflow | Low | Medium | Drop old events with warning marker |
| Focus loss on app crash | Medium | Medium | Automatic focus recovery to topmost app |
| Clipboard corruption | Low | High | CRC32 + ownership tracking prevents race |

---

## Timeline

**Session 1** (now):
- ✅ Create Phase 22 plan (this document)
- 🟡 Task 1: Window lifecycle (~90 min)
- 🟡 Task 2: Clipboard & sandbox (~75 min)
- 🟡 Task 3: IPC & events (~90 min)

**Session 2** (next):
- 🟡 Task 4: Surface rendering (~90 min)
- 🟡 Task 5: Shell commands (~60 min)
- 🟡 Task 6: Testing & integration (~90 min)
- ✅ Create Phase 22 final report

---

## References

- [RAYOS_OVERVIEW_2026.md](/RAYOS_OVERVIEW_2026.md): Phase 22 description + context
- [PHASE_21_FINAL_REPORT.md](/PHASE_21_FINAL_REPORT.md): Foundation just completed
- [crates/kernel-bare/src/rayapp.rs](/crates/kernel-bare/src/rayapp.rs): Starting point
- [crates/kernel-bare/src/linux_presentation.rs](/crates/kernel-bare/src/linux_presentation.rs): Rendering foundation
- [crates/kernel-bare/src/shell.rs](/crates/kernel-bare/src/shell.rs): Command dispatcher

---

**Phase 22 Planning Complete** ✅

Ready to proceed with Task 1 implementation on next step.
