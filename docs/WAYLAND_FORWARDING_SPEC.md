# RayOS Wayland Forwarding Specification

Status: **draft (v0 spec)**

Purpose: Define the mechanism for forwarding guest Wayland surfaces to the RayOS host, enabling guest applications to appear as native RayOS windows.

---

## 1) Scope (v0)

This specification defines the initial mechanism for Wayland forwarding. The primary goal is to have a working, minimal implementation that can display a single guest Wayland surface (the entire guest desktop) as a surface in RayOS.

- **Goal:** Display a single guest Wayland desktop as a RayOS surface.
- **Mechanism:** Use a simple, well-supported transport mechanism.
- **Guest:** The initial focus is on the Linux subsystem.

Non-goals (v0):
-   Multi-window support (i.e., mapping individual Wayland surfaces to individual RayOS windows).
-   High-performance, low-latency graphics.
-   Clipboard integration, HiDPI scaling, and other advanced features.

---

## 2) Proposed Transport Mechanism: virtio-gpu

For the initial implementation, we will use `virtio-gpu` with scanout capture.

### 2.1 Rationale

-   **Simplicity:** `virtio-gpu` is a mature, well-understood, and widely supported standard. Capturing the framebuffer is a straightforward way to get the graphical output of the guest.
-   **Minimal Guest-Side Changes:** Using `virtio-gpu` requires minimal changes to the guest OS. The guest's Wayland compositor (e.g., Weston) will render to the `virtio-gpu` device as if it were a normal display.
-   **"Keep it minimal for milestone 1":** This approach directly aligns with the directive to keep the initial implementation simple.

### 2.2 Alternative Considered: virtio-wayland

A `virtio-wayland` style bridge would provide a more direct and potentially more performant path for Wayland communication. It would also make multi-window support easier to implement in the future.

However, it is also significantly more complex to implement, both on the host and guest side. It would require a custom virtio device and a corresponding driver in the guest. This complexity makes it unsuitable for the initial milestone.

---

## 3) Implementation Plan

### 3.1 Guest-Side (Linux Subsystem)

1.  **Configure the VM:** The Linux VM will be configured with a `virtio-gpu` device.
2.  **Weston:** The Weston compositor in the guest will be configured to use the `virtio-gpu` device as its output. No custom Weston backend is required.

### 3.2 Host-Side (RayOS)

1.  **QEMU:** QEMU will be configured to expose the `virtio-gpu` framebuffer. This can be done by mapping the device's memory to a file or a shared memory region.
2.  **RayOS Compositor:** The RayOS compositor will read the framebuffer data from the file or shared memory region.
3.  **Rendering:** The RayOS compositor will render the framebuffer data as a texture on a surface.

### 3.3 Communication

The communication is one-way: from the guest to the host. The host reads the framebuffer data that the guest writes.

---

## 4) Future Work

-   **Multi-window support:** A `virtio-wayland` bridge will be investigated for a future milestone to enable seamless multi-window support.
-   **Performance:** Performance optimizations, such as using shared memory with explicit synchronization, will be considered in a future milestone.

---

## 5) Related Documents

-   [LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md)
-   [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)
