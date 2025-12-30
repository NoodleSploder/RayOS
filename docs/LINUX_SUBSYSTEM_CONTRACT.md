# RayOS Linux Subsystem Contract (Option D)

This document defines the **non-negotiable contract** for running Linux as a **subsystem of RayOS**.

Core principle:

> **RayOS has full control over the Linux subsystem environment.**
> RayOS is the admin, the user, and the pilot.
> Linux is a managed guest runtime that is completely driven by RayOS.

This contract is the authoritative source for Option D implementation decisions.

---

## 1) Goals

- Run Linux userspace for **app compatibility** while RayOS remains the host OS.
- Provide a **Wayland-first** graphical path.
- Enable two UX milestones:
  1) **Embedded desktop surface** (single surface/window) for baseline compatibility.
  2) **Native-window mapping** (Linux apps appear as RayOS windows) for deep integration.

Additional goal (persistence across RayOS reboots):

- The Linux subsystem is a **long-lived VM instance** managed by RayOS. RayOS reboot must **reattach to the same instance** (stable identity + persistent storage) and should **resume the same session** when a saved-state exists.

Additional goal (boot behavior + invisibility by default):

- By default, RayOS should **boot/resume the Linux VM during RayOS boot** (background), but Linux is **not presented** and **not user-interactive** until RayOS explicitly enables presentation and input routing.

---

## 2) Non-goals (explicitly out of scope)

- Linux becoming the host OS.
- “Dual control” where Linux directly owns devices.
- Granting Linux direct access to host hardware (GPU, keyboard, mouse, disks) outside RayOS policy.

---

## 3) Authority model (who controls what)

### 3.1 RayOS is the authority for everything
RayOS is responsible for and controls:

- **Lifecycle**: create/start/stop/restart/suspend/snapshot/destroy the subsystem.
- **Continuity**: maintain a stable Linux VM instance identity across RayOS reboots and define resume/restore behavior.
- **Boot policy**: decide whether Linux is started/resumed at RayOS boot; when started, keep it hidden/non-interactive until explicitly presented.
- **Resources**: CPU caps, memory caps, storage quotas, device exposure.
- **Input**: keyboard/mouse/touch/IME events. Linux receives input only through RayOS routing.
- **Graphics**: what is rendered, how it is presented, and what appears as a RayOS surface/window.
- **Terminal / text I/O**: what Linux serial console is shown, logged, throttled, and injected.
- **Filesystem**: what paths are visible, read-only vs read-write, what persists.
- **Networking**: whether it exists at all; egress/ingress policies.
- **Time/entropy** (when needed): policy-controlled exposure to reduce fingerprinting or enforce determinism.

### 3.2 Linux is a managed runtime
Linux is treated like a sandboxed compatibility process. It:

- Boots only when RayOS instructs it to boot.
- Sees only the virtual devices RayOS exposes.
- Does not get raw host privileges; its “root” is still inside a RayOS-governed boundary.

---

## 4) Device & integration model (what Linux sees)

Linux runs as a guest (VM first; other isolation mechanisms are not assumed here).

Minimum virtual device set (subject to change, but the *principle* must hold):

- **Console/serial**: for logs, readiness markers, and command/control.
- **virtiofs / virtio-blk**: storage presented from RayOS with explicit policy.
- **virtio-net**: optional and policy-gated (default should be “off” until explicitly enabled).
- **virtio-input**: input delivered only via RayOS routing.
- **virtio-gpu** (or equivalent): graphics output always mediated.

Linux must never require direct host device pass-through for the baseline milestones.

---

## 5) Control plane (RayOS drives Linux)

Linux must run a **guest agent** that accepts commands *from RayOS*.

### 5.1 Control channel requirements
- Unidirectional authority: **RayOS → Guest** commands; guest only replies with results/telemetry.
- The channel must be:
  - **Deterministic** where possible (stable markers, bounded timeouts).
  - **Auditable** (log every command and response on the host side).
  - **Robust** (versioning, schema, and explicit error codes).

### 5.2 Minimum command surface (initial)
- `health` / `ready` handshake.
- `launch` (start an app by command or desktop id).
- `list_surfaces` / `surface_created` events (to support embedding and native windows).
- `shutdown`.

The exact wire format can be simple (line-based) initially, but must be versioned.

---

## 6) Graphics contract (Wayland-first)

### 6.1 Embedded desktop surface (milestone 1)
- RayOS presents **one** Linux desktop surface as a single RayOS surface/window.
- RayOS decides:
  - where it appears,
  - its size/scale,
  - whether it is visible,
  - when it is destroyed.

### 6.2 Native-window mapping (milestone 2)
- Guest Wayland surfaces map to RayOS windows.
- RayOS is authoritative for:
  - focus,
  - input routing,
  - resize and scale policy,
  - window lifetime.

Linux may request surface roles (e.g., toplevel/popup), but RayOS may deny/override.

---

## 7) Input contract

- All input originates from RayOS.
- Linux receives input only through RayOS-approved virtual devices.
- RayOS controls focus and can:
  - suppress input,
  - reroute input,
  - inject input,
  - record/replay input (if desired by policy).

### 7.1 Hidden-by-default interaction requirement

- When Linux is not presented, RayOS must not route user input to Linux unless explicitly requested.
- “Presented” is the gate that enables interactive input routing.

---

## 8) Filesystem & persistence contract

- RayOS is the source of truth for persistent storage.
- Default posture:
  - Linux root filesystem can be **immutable** or ephemeral.
  - Persistent user data lives in RayOS-managed volumes.
- All host-visible mounts must be explicitly defined and policy-checked.

### 8.1 VM continuity requirements (non-negotiable)

- RayOS must not treat the Linux subsystem as “new every boot.”
- There must be a stable VM identity concept (name/id) that selects the same backing storage and policy across RayOS reboots.
- If RayOS supports VM state persistence, it must prefer **resume from saved-state** on boot; otherwise, it must cold-boot the **same** persistent VM disk(s).

---

## 9) Networking contract

- Default posture: **no networking** unless explicitly enabled by RayOS policy.
- If enabled, RayOS controls:
  - DNS,
  - egress restrictions,
  - inbound restrictions,
  - per-app/network namespace policy (future).

---

## 10) Observability & testability

- Linux guest must emit stable readiness markers over serial so host tests can assert progress.
- Host tooling must be able to:
  - boot guest headlessly,
  - detect “agent ready”,
  - launch an app,
  - observe surface creation,
  - clean shutdown.

---

## 11) Security invariants (must always hold)

- Linux never bypasses RayOS policy to reach host devices.
- All integration is mediated through RayOS-controlled virtual devices and channels.
- Guest agent accepts commands only from RayOS and should be designed with least privilege.

---

## 12) Open choices (allowed to vary without violating the contract)

These can change as implementation evolves, as long as the authority model remains intact:

- VM backend details (KVM acceleration when available).
- The exact virtio device set and configuration.
- The exact wire format of the control plane (line-based vs binary), provided it is versioned.
- The initial guest distro/rootfs choice.
