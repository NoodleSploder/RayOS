
Minimum Viable Hardware Target
Purpose

Define a single, known-good platform to validate RayOS architecture rapidly.

1. MVP Hardware Profile (Recommended)

Architecture

x86_64 host

GPU

Discrete NVIDIA GPU

Persistent compute capable

Mature tooling for early development

CPU

General-purpose, assists only

Memory

≥ 32 GB RAM

Camera

UVC-compatible webcam

Display

Standard HDMI / DisplayPort

2. Rationale

Fast iteration

Stable virtualization

Mature GPU drivers

Avoids BSP and bootloader complexity early

3. Deferred Targets

Integrated SoCs (Jetson-class)

Embedded robotics platforms

Multi-GPU distributed fabrics

These are Phase 2+ validation, not MVP blockers.

4. Hard Requirements

GPU supports persistent execution

Camera input available at boot

IOMMU available (for isolation later)

5. Non-Goals

Battery optimization

Mobile form factors

Broad hardware compatibility
