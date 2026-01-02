# RayOS Windows GPU Contract

Status: **draft (v0 spec)**

Purpose: Define the long-term vision and phased approach for GPU acceleration in the Windows subsystem.

---

## 1) Long-Term Vision: Paravirtualized GPU

The ultimate goal is to provide the Windows guest with a high-performance, paravirtualized GPU that is compatible with the Windows Display Driver Model (WDDM). This will allow Windows applications, including those that use 3D graphics, to run efficiently under RayOS.

RayOS, as the host, will remain in full control of the physical GPU. The guest will interact with a virtual GPU, and RayOS will be responsible for scheduling and executing the rendering commands on the physical hardware.

---

## 2) Phased Approach

### Phase 1: Basic Display (Current State)

-   **Mechanism:** `virtio-gpu` (`virtio-vga`) without 3D acceleration.
-   **Guest Experience:** The guest has a basic display adapter. 2D graphics are accelerated, but 3D applications will fall back to software rendering.
-   **Status:** ✅ Done. This is the current state of the Windows subsystem.

### Phase 2: Paravirtualized 3D Acceleration (Proposed)

-   **Mechanism:** `virtio-gpu` with 3D acceleration enabled (e.g., using VirGL).
-   **Guest Experience:** The guest will have a WDDM-compatible `virtio-gpu` driver. 3D applications will be able to use hardware acceleration.
-   **Implementation:**
    1.  **Host:** Configure QEMU to enable the `virgl` option for the `virtio-gpu` device. The RayOS host will need to have a compatible VirGL rendering backend.
    2.  **Guest:** A WDDM-compliant `virtio-gpu` driver with 3D support needs to be installed in the Windows guest. The open-source `virtio-win` project provides such a driver.

### Phase 3: RayOS-Native GPU Scheduling (Future)

-   **Mechanism:** A custom RayOS GPU scheduler that directly manages the command buffers from the `virtio-gpu` device.
-   **Guest Experience:** Seamless, high-performance graphics.
-   **Implementation:** This is a long-term research topic that will require significant effort in both the host and guest.

---

## 3) v0 Contract: `virtio-gpu` with VirGL

For the next milestone, the contract is to use `virtio-gpu` with VirGL as the bridge for 3D acceleration.

-   **Host Responsibilities:**
    -   Provide a stable VirGL rendering backend.
    -   Manage the allocation of GPU resources to the guest.
-   **Guest Responsibilities:**
    -   Use a standard `virtio-gpu` WDDM driver.
-   **Communication:** The guest sends 3D commands (in the form of Gallium3D command buffers) to the host via the `virtio-gpu` device. The host translates these commands and executes them on the physical GPU.

---

## 4) Related Documents

-   [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md)
-   [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)
