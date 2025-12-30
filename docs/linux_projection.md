
Linux Compatibility Projection
Purpose

Enable RayOS to present and control a Linux desktop without ceding authority.

Linux is a projected environment, not the host OS.

1. Core Concept

RayOS:

Owns perception, intent, and scheduling

Treats Linux as a rendered surface + input sink

2. Architecture
RayOS Host
 ├── Persistent Kernel Loop
 ├── Perception & Intent
 ├── Projection Layer
 │    ├── Frame Ingest
 │    ├── Input Injection
 │    └── Metadata Channel (optional)
 └── Linux VM
      ├── Display Server
      └── Guest Agent (optional)

3. Frame Projection

Linux renders normally

Output is captured to shared memory

RayOS composites or displays the frame

RayOS never depends on Linux UI state for correctness.

4. Input Injection

RayOS injects:

Pointer movement

Button press

Keyboard input

These are execution effects, not control logic.

5. Metadata Channel (MVP+)

Optional but powerful:

Guest agent publishes:

Window geometry

Active window

Widget hierarchy (if available)

Transport options:

virtio-serial

vsock

shared memory ring buffer

6. Object Identity Mapping

RayOS assigns stable object IDs even if Linux re-creates widgets.

Linux identity is advisory, not authoritative.

7. Failure Isolation

Linux crash ≠ RayOS crash

Linux freeze ≠ loss of perception

Projection can be restarted independently

8. Non-Goals

Running arbitrary Linux drivers in RayOS

POSIX compliance

Making Linux “primary”
