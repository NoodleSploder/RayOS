# RayOS Quick Start Guide

**Last Updated**: January 2026

---

## Prerequisites

### Linux (Recommended)

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y \
    qemu-system-x86 \
    ovmf \
    xorriso \
    dosfstools \
    mtools \
    python3

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Windows (via WSL2)

1. Install WSL2 with Ubuntu
2. Follow Linux instructions above inside WSL2

---

## Build and Run

### 1. Clone Repository

```bash
git clone https://github.com/your-org/RayOS.git
cd RayOS
```

### 2. Build Bootable Images

```bash
./scripts/build-iso.sh
```

This creates:
- `build/rayos.iso` - Bootable ISO
- `build/rayos-universal-usb.img` - USB image

### 3. Run in QEMU

**Graphical (Interactive):**
```bash
./scripts/run-ui-shell.sh
```

**Headless (Automated Test):**
```bash
./scripts/test-ui-shell-headless.sh
```

---

## What You'll See

The RayOS desktop with:

1. **System Status** window - Hardware and subsystem status
2. **AI Assistant** window - Chat interface with built-in LLM

### Mouse Controls

| Action | Effect |
|--------|--------|
| Click title bar | Start dragging window |
| Drag edges/corners | Resize window |
| Double-click title | Maximize/restore |
| Click × button | Close window |
| Click input area | Activate text input |

### Keyboard

| Key | Effect |
|-----|--------|
| Type | Enter text (when input active) |
| Enter | Submit text |
| Escape | Deactivate text input |
| Backspace | Delete character |

---

## Directory Structure

```
RayOS/
├── crates/
│   ├── kernel-bare/     # Main kernel
│   ├── bootloader/      # UEFI bootloader
│   └── ...
├── scripts/             # Build and test scripts
├── docs/                # Documentation
└── build/               # Generated artifacts
```

---

## Common Commands

```bash
# Build kernel only
cd crates/kernel-bare
cargo build --features ui_shell,serial_debug --target x86_64-unknown-none --release

# Run tests
./scripts/test-ui-shell-headless.sh

# View serial output
cat build/serial-ui-shell-headless.log
```

---

## Troubleshooting

### QEMU not found
```bash
# Install QEMU
sudo apt-get install qemu-system-x86
```

### OVMF not found
```bash
# Install OVMF
sudo apt-get install ovmf

# Or specify path
OVMF_CODE=/path/to/OVMF_CODE.fd ./scripts/run-ui-shell.sh
```

### Build fails
```bash
# Update Rust toolchain
rustup update
rustup target add x86_64-unknown-none
```

---

## Next Steps

- [Build Guide](BUILD_GUIDE.md) - Detailed build options
- [UI Framework](RAYOS_UI_FRAMEWORK.md) - UI system documentation
- [App Development](development/APP_DEVELOPMENT.md) - Building apps for RayOS
