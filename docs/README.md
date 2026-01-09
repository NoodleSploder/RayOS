# RayOS Documentation

Welcome to the RayOS documentation. This index provides quick access to all resources.

---

## Getting Started

| Document | Description |
|----------|-------------|
| [README](../README.md) | Project overview and quick start |
| [Quick Start](QUICKSTART.md) | First-time setup guide |
| [Build Guide](BUILD_GUIDE.md) | Detailed build instructions |
| [Roadmap](ROADMAP.md) | Current development roadmap |

---

## Architecture & Design

| Document | Description |
|----------|-------------|
| [System Architecture](SYSTEM_ARCHITECTURE.md) | End-to-end system design |
| [UI Framework](RAYOS_UI_FRAMEWORK.md) | Native UI implementation |
| [RayOS Overview](RAYOS_OVERVIEW_2026.md) | High-level vision and status |

---

## Subsystems

| Document | Description |
|----------|-------------|
| [Linux Subsystem Design](LINUX_SUBSYSTEM_DESIGN.md) | Linux VM integration |
| [Linux Subsystem Contract](LINUX_SUBSYSTEM_CONTRACT.md) | Linux API contract |
| [Windows Subsystem Design](WINDOWS_SUBSYSTEM_DESIGN.md) | Windows VM integration |
| [Windows Subsystem Contract](WINDOWS_SUBSYSTEM_CONTRACT.md) | Windows API contract |

---

## Application Development

| Document | Description |
|----------|-------------|
| [App Development](development/APP_DEVELOPMENT.md) | Building RayOS applications |
| [Framework Roadmap](development/FRAMEWORK_ROADMAP.md) | UI framework future plans |
| [Contributing](development/CONTRIBUTING.md) | Contribution guidelines |

---

## Boot & Installation

| Document | Description |
|----------|-------------|
| [Bootloader Design](BOOTLOADER_CHAINLOADING.md) | Boot process details |
| [Installer Specification](INSTALLER_AND_BOOT_MANAGER_SPEC.md) | Installation flow |
| [Disk Layout](DISK_LAYOUT_AND_PERSISTENCE.md) | Storage organization |

---

## Reference

| Document | Description |
|----------|-------------|
| [Policy Configuration](POLICY_CONFIGURATION_SCHEMA.md) | System policy settings |
| [Observability](OBSERVABILITY_AND_RECOVERY.md) | Logging and recovery |
| [Security Model](SECURITY_THREAT_MODEL.md) | Security considerations |

---

## Archive

Historical documentation is organized in subdirectories:

- `archive/` - Session reports, old status docs
- `phases/` - Phase planning and completion reports

---

## Quick Commands

```bash
# Build bootable images
./scripts/build-iso.sh

# Run interactive UI shell
./scripts/run-ui-shell.sh

# Run headless tests
./scripts/test-ui-shell-headless.sh
```

---

## Source Code

Key directories in the codebase:

| Path | Description |
|------|-------------|
| `crates/kernel-bare/src/` | Main kernel source |
| `crates/kernel-bare/src/ui/` | UI framework |
| `crates/bootloader/` | UEFI bootloader |
| `scripts/` | Build and test scripts |
