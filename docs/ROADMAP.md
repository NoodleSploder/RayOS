# RayOS Roadmap

**Last Updated**: January 2026

---

## Current Focus: UI Framework & App Platform

### Completed ✅

| Feature | Description |
|---------|-------------|
| Kernel Boot | x86_64 and aarch64 UEFI boot |
| Framebuffer | Direct framebuffer rendering |
| Window Manager | Create, move, resize, focus windows |
| Compositor | Z-order compositing, decorations |
| Mouse Input | PS/2 driver with cursor |
| Keyboard Input | Text input handling |
| Linux VM | Running as managed guest |
| Local AI | In-kernel LLM inference |
| Process Explorer | Graphical process/system monitor |
| System Log | In-kernel event journal for diagnostics |
| Widget Library | Button, Label, TextInput widgets |
| Layout System | VStack, HStack, Grid containers |
| VM Window | Linux desktop as native window with input routing |
| Linux Graphics | Resolution tracking, FPS overlay, bilinear scaling |
| App SDK | AppDescriptor, AppContext, lifecycle hooks, example apps |
| VS Code Extension | Build commands, snippets, .rayapp syntax, QEMU integration |
| Windows VM | Windows subsystem with UEFI, TPM, Hyper-V enlightenments |
| Package Format | .rayapp package structure, loader, and shell commands |

### In Progress 🟡

| Feature | Target | Notes |
|---------|--------|-------|
| App Store | Q4 2026 | App discovery and installation |

### Planned 📋

| Feature | Target | Notes |
|---------|--------|-------|
| App Store | Q4 2026 | App discovery and installation |

---

## Milestones

### M1: Native Linux Desktop (Q1 2026)

Linux VM runs in a native RayOS window with:
- virtio-gpu scanout
- Input routing
- Window decorations

### M2: App Framework Alpha (Q2 2026)

First public SDK release:
- Widget library
- Layout system
- Documentation

### M3: VS Code Integration (Q2 2026)

Developer tooling:
- Project templates
- Build tasks
- Debug adapter

### M4: Standalone Deployment (Q3 2026)

Production-ready installation:
- Installer
- Update mechanism
- Recovery mode

---

## Technical Debt

| Item | Priority | Notes |
|------|----------|-------|
| Font rendering | Low | Currently 8x16 bitmap |
| Animations | Low | Window transitions |
| Multi-monitor | Medium | Future hardware support |

---

## See Also

- [Framework Roadmap](development/FRAMEWORK_ROADMAP.md) - Detailed app framework plans
- [App Development](development/APP_DEVELOPMENT.md) - Building apps for RayOS
