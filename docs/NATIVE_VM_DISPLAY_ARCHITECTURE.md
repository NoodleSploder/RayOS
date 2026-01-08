# RayOS Native VM Display Architecture

**Status**: Production Architecture
**Last Updated**: January 8, 2026

---

## Executive Summary

RayOS implements a **native hypervisor** that runs Linux (and potentially Windows) virtual machines **inside the RayOS kernel itself**. The guest VM's framebuffer is composited directly into the RayOS framebuffer, allowing seamless switching between the RayOS UI and the Linux desktop.

**This is NOT a QEMU bridge. RayOS IS the hypervisor.**

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Physical Hardware                           │
│  (CPU with VT-x/AMD-V, GPU, Memory, Keyboard, Display)              │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          RayOS Kernel                               │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    RayOS Hypervisor (VMM)                     │  │
│  │  • Intel VT-x / AMD-V virtualization                          │  │
│  │  • VMCS management, EPT/NPT paging                            │  │
│  │  • virtio-gpu scanout capture                                 │  │
│  │  • virtio-input keyboard/mouse forwarding                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                          │              │                           │
│              ┌───────────┘              └────────────┐              │
│              ▼                                       ▼              │
│  ┌─────────────────────────┐          ┌─────────────────────────┐  │
│  │     RayOS Native UI     │          │   Linux Guest VM        │  │
│  │  • Text chat interface  │          │   • Full Linux kernel   │  │
│  │  • AI query/response    │          │   • Desktop environment │  │
│  │  • System status        │          │   • Applications        │  │
│  │  • GPU direct render    │          │   • virtio-gpu output   │  │
│  └─────────────────────────┘          └─────────────────────────┘  │
│              │                                       │              │
│              └───────────────┬───────────────────────┘              │
│                              ▼                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                  Framebuffer Compositor                       │  │
│  │  • Guest scanout → RayOS framebuffer blit                     │  │
│  │  • Toggle between RayOS UI and Linux desktop                  │  │
│  │  • Smooth presentation state transitions                      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                      │
└──────────────────────────────┼──────────────────────────────────────┘
                               ▼
                    ┌─────────────────────┐
                    │   Physical Display  │
                    │   (GPU Framebuffer) │
                    └─────────────────────┘
```

---

## Key Design Principles

### 1. RayOS IS the Hypervisor
- RayOS kernel includes a full Type-1 hypervisor implementation
- Uses Intel VT-x (VMCS, EPT) or AMD-V (VMCB, NPT)
- The hypervisor module is ~10,000 lines of Rust code
- Located at: `crates/kernel-bare/src/hypervisor.rs`

### 2. No External Dependencies
- **Does NOT rely on QEMU** for any functionality
- **Does NOT spawn separate VM processes** (no bridge mode)
- Works identically on bare metal and during development
- All virtualization is handled by RayOS kernel code

### 3. Framebuffer Compositing
- Guest VM outputs via virtio-gpu
- RayOS captures the guest scanout buffer
- Compositor blits guest framebuffer into RayOS display
- Single physical display shows either RayOS UI or Linux desktop

### 4. Hardware Keyboard Input
- Keyboard scancodes handled at lowest level
- Toggle keys work regardless of current display mode
- Input can be routed to RayOS or forwarded to guest via virtio-input

---

## Toggle Mechanism

### Keyboard Controls (Hardware-Level)

| Key | Scancode | Action |
|-----|----------|--------|
| **Backtick (`)** | 0x29 | Toggle between RayOS UI and Linux Desktop |
| **F11** | 0x57 | Show Linux Desktop (enter Presented mode) |
| **F12** | 0x58 | Hide Linux Desktop (return to RayOS UI) |

### How It Works

1. **Scancode Handler** (`keyboard_handle_scancode`)
   - Receives raw keyboard scancodes from i8042 controller
   - Checks for toggle keys BEFORE any ASCII conversion
   - Works even when guest desktop is displayed (Presented mode)

2. **Presentation State Machine**
   ```
   Hidden ──────► Presented
      ▲              │
      │              │
      └──────────────┘
   ```
   - `Hidden`: RayOS UI is displayed, guest runs in background
   - `Presented`: Linux desktop fills the screen, RayOS UI hidden

3. **Toggle Implementation** (main.rs:8367-8407)
   ```rust
   // Hide: F12 or backtick when Presented
   if presentation_state == Presented && (sc == 0x58 || sc == 0x29) {
       set_presentation_state(Hidden);
   }
   
   // Show: F11 or backtick when Hidden
   if presentation_state != Presented && (sc == 0x57 || sc == 0x29) {
       set_presentation_state(Presented);
   }
   ```

---

## Guest Surface Management

### Surface Publication
```rust
pub struct GuestSurface {
    pub width: u32,
    pub height: u32,
    pub stride_px: u32,
    pub bpp: u32,           // Bits per pixel (32 = XRGB8888)
    pub backing_phys: u64,  // Physical address of guest framebuffer
}
```

### Compositor Flow
1. Guest VM writes to virtio-gpu framebuffer
2. Hypervisor captures SET_SCANOUT command
3. Guest surface is published to RayOS compositor
4. When in Presented mode, compositor blits guest buffer to display
5. Frame sequence counter bumped for vsync coordination

### Location
- `crates/kernel-bare/src/guest_surface.rs` - Surface management
- `crates/kernel-bare/src/dev_scanout.rs` - Development/test producer (synthetic)
- `crates/kernel-bare/src/hypervisor.rs` - Real virtio-gpu capture

---

## virtio Device Support

### virtio-gpu
- Captures guest display output
- Translates SET_SCANOUT into GuestSurface publication
- Supports XRGB8888 pixel format
- Resolution negotiated with guest

### virtio-input
- Forwards keyboard scancodes to guest
- Forwards mouse events to guest
- Only active when in Presented mode
- Toggle keys (backtick, F11, F12) intercepted before forwarding

### virtio-blk
- Provides storage to guest VM
- Backed by in-memory or disk image

---

## Feature Flags

| Feature | Description |
|---------|-------------|
| `vmm_hypervisor` | Enable embedded hypervisor (Intel VT-x/AMD-V) |
| `vmm_virtio_input` | Enable virtio-input forwarding to guest |
| `dev_scanout` | Enable synthetic test scanout (development only) |

### Build Commands

**Production (with real hypervisor):**
```bash
RAYOS_KERNEL_FEATURES=vmm_hypervisor,vmm_virtio_input ./scripts/test-boot.sh
```

**Development (synthetic test pattern):**
```bash
RAYOS_KERNEL_FEATURES=dev_scanout ./scripts/test-boot.sh
```

---

## User Acceptance Testing

### Test Scenario: Native Linux Desktop Toggle

1. **Start RayOS:**
   ```bash
   RAYOS_KERNEL_FEATURES=dev_scanout ./scripts/test-boot.sh
   ```

2. **Verify RayOS UI appears** (text interface, status display)

3. **Press backtick (`) or F11** to show Linux desktop
   - Should see Linux desktop fill the screen
   - RayOS UI is hidden

4. **Press backtick (`) or F12** to return to RayOS
   - RayOS UI reappears
   - Linux continues running in background

5. **Toggle repeatedly** - should be instantaneous

### Expected Behavior
- ✅ Backtick toggles display mode
- ✅ F11 always shows Linux desktop
- ✅ F12 always returns to RayOS
- ✅ Linux VM continues running when hidden
- ✅ Keyboard input goes to correct destination
- ✅ Works on real hardware (no QEMU dependencies)

---

## Files Reference

| File | Purpose |
|------|---------|
| `hypervisor.rs` | VT-x/AMD-V hypervisor, VMCS, EPT, virtio emulation |
| `guest_surface.rs` | Guest framebuffer surface management |
| `dev_scanout.rs` | Synthetic test pattern producer |
| `main.rs:8345-8450` | Keyboard scancode handling and toggle logic |
| `main.rs:12361-12390` | Backup ASCII-level toggle (dev_scanout only) |

---

## Design Decisions

### Why Not QEMU Bridge?
1. **Performance**: Direct scanout blit is faster than VNC/SPICE
2. **Simplicity**: Single process, single display, no IPC
3. **Portability**: Works on bare metal without host OS
4. **User Experience**: Seamless, instant toggle

### Why Backtick?
1. **Accessible**: Easy to reach on all keyboards
2. **Distinctive**: Not commonly used in applications
3. **Gaming standard**: Similar to console toggle in games
4. **Fallback**: F11/F12 as alternatives

### Why Scancode-Level Toggle?
1. **Always works**: Even when guest captures keyboard
2. **Low latency**: No ASCII conversion delay
3. **Reliable**: Direct hardware path

---

## Future Enhancements

1. **Multi-monitor support** - Show RayOS on one display, Linux on another
2. **Picture-in-picture** - Linux desktop as overlay window
3. **Windows guest** - Same architecture, different virtio drivers
4. **GPU passthrough** - Direct GPU access for guest
5. **Snap layouts** - Split screen between RayOS and guest

---

## Summary

RayOS is not just an operating system - it's a **hypervisor-based platform** that:

- Runs Linux VMs **natively** inside the RayOS kernel
- Composites guest display **directly** into the framebuffer
- Provides **instant toggle** between RayOS UI and Linux desktop
- Works **identically** on bare metal and during QEMU-based development
- Requires **zero external dependencies** for VM functionality

The architecture ensures that pressing backtick on a physical keyboard connected to real hardware will toggle the display exactly the same way as pressing backtick in QEMU during development.

---

*Document Version: 1.0*
*RayOS Development Team*
