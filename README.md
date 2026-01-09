# RayOS

**An AI-Native Operating System with Bicameral Architecture**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-21-blue)]()
[![License](https://img.shields.io/badge/license-MIT-green)]()

---

## Overview

RayOS is an experimental, Rust-based, UEFI-bootable operating system implementing a **bicameral kernel architecture**—combining real-time GPU processing (System 1) with LLM-based reasoning (System 2) for AI-first computing.

### Key Features

- **Native UI Framework** — Desktop-class windowing system for RayOS applications
- **Managed Subsystems** — Linux and Windows run as managed VMs with seamless integration
- **AI-First Design** — Built-in LLM engine for natural language interaction
- **Cross-Platform** — Supports x86_64 and aarch64 architectures

---

## Current Status (January 2026)

| Component | Status |
|-----------|--------|
| **Kernel** | ✅ Running on x86_64 and aarch64 |
| **UI Framework** | ✅ Window manager, compositor, input handling |
| **Linux Subsystem** | ✅ Running as managed VM |
| **Windows Subsystem** | 🟡 In development |
| **Local AI (LLM)** | ✅ In-kernel inference |
| **App Framework** | 🟡 API design phase |

### What Works Today

- Boot from USB/ISO on real hardware or QEMU
- Native windowed desktop with mouse and keyboard input
- Draggable, resizable windows with decorations
- System Status and AI Assistant windows
- Linux desktop presented as native RayOS window
- Local AI responses via built-in LLM

---

## Quick Start

### Prerequisites

**Linux (recommended):**
```bash
# Debian/Ubuntu
sudo apt-get install -y qemu-system-x86 ovmf xorriso dosfstools mtools python3

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build and Run

```bash
# Clone the repository
git clone https://github.com/your-org/RayOS.git
cd RayOS

# Build bootable images
./scripts/build-iso.sh

# Run in QEMU (graphical)
./scripts/run-ui-shell.sh

# Run headless test
./scripts/test-ui-shell-headless.sh
```

### Controls

| Input | Action |
|-------|--------|
| Mouse | Window interaction, drag, resize |
| Click title bar | Move window |
| Click edges/corners | Resize window |
| Double-click title | Maximize/restore |
| Click input field | Activate text input |
| Enter | Submit text |
| Escape | Deactivate text input |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   RayOS Kernel                              │
├────────────────────┬────────────────────────────────────────┤
│  System 1 (GPU)    │   System 2 (LLM)                       │
│  Real-time reflex  │   Cognitive reasoning                  │
└────────────────────┴────────────────────────────────────────┘
         │                        │
         └───────────┬────────────┘
                     ▼
            ┌─────────────────┐
            │   Conductor     │
            │ (Orchestration) │
            └─────────────────┘
                     │
    ┌────────────────┼────────────────┐
    ▼                ▼                ▼
┌─────────┐   ┌─────────────┐   ┌─────────────┐
│ Native  │   │   Linux     │   │  Windows    │
│   UI    │   │   Guest     │   │   Guest     │
└─────────┘   └─────────────┘   └─────────────┘
```

### Core Components

| Component | Description |
|-----------|-------------|
| **Bootloader** | UEFI bootloader for x86_64/aarch64 |
| **Kernel** | Bare-metal Rust kernel with bicameral design |
| **UI Framework** | Native windowing, compositing, input |
| **Conductor** | Task orchestration and scheduling |
| **VMM** | In-kernel hypervisor for guest VMs |

---

## Repository Structure

```
RayOS/
├── crates/                    # Rust workspace
│   ├── kernel-bare/          # Main kernel
│   │   └── src/ui/           # UI Framework
│   ├── bootloader/           # UEFI bootloader
│   ├── volume/               # Storage management
│   └── cortex/               # AI/LLM components
├── scripts/                   # Build and test scripts
├── docs/                      # Documentation
│   ├── development/          # Developer guides
│   └── phases/               # Historical phase reports
└── build/                     # Build artifacts (generated)
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [Getting Started](docs/QUICKSTART.md) | First-time setup guide |
| [Build Guide](docs/BUILD_GUIDE.md) | Detailed build instructions |
| [System Architecture](docs/SYSTEM_ARCHITECTURE.md) | Technical architecture |
| [UI Framework](docs/RAYOS_UI_FRAMEWORK.md) | Native UI documentation |
| [App Development](docs/development/APP_DEVELOPMENT.md) | Building RayOS apps |
| [Contributing](docs/development/CONTRIBUTING.md) | Contribution guidelines |

---

## Developing for RayOS

RayOS provides a native application framework for building desktop applications. See the [App Development Guide](docs/development/APP_DEVELOPMENT.md) for details.

```rust
// Example: Creating a RayOS window
let window = window_manager::create_window(
    b"My App",
    100, 100,    // position
    400, 300,    // size
    WindowType::Normal,
);
```

---

## License

RayOS is released under the MIT License. See [LICENSE](LICENSE) for details.

---

## Acknowledgments

RayOS is an experimental research project exploring AI-native operating system design.
