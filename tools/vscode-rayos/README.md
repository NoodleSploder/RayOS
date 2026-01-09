# RayOS VS Code Extension

Development tools for building RayOS applications.

## Features

- **Build Commands** - Build kernel and apps from the command palette
- **QEMU Integration** - Run RayOS directly from VS Code
- **Code Snippets** - Quick templates for App SDK code
- **Syntax Highlighting** - `.rayapp` manifest file support
- **Task Provider** - Pre-configured build tasks

## Commands

| Command | Description |
|---------|-------------|
| `RayOS: Build Kernel` | Build the RayOS kernel |
| `RayOS: Run in QEMU` | Launch RayOS in QEMU |
| `RayOS: Create New App` | Scaffold a new application |
| `RayOS: Clean Build` | Remove build artifacts |
| `RayOS: Open Documentation` | Browse RayOS docs |

## Snippets

| Prefix | Description |
|--------|-------------|
| `rayos-app` | Full app struct with App trait |
| `rayos-descriptor` | AppDescriptor constant |
| `rayos-frame` | on_frame method |
| `rayos-event` | on_event method |
| `rayos-button` | Button with hover state |
| `rayos-use` | Import App SDK types |
| `ctx-fill` | Fill rectangle |
| `ctx-text` | Draw text |
| `ctx-rect` | Draw rectangle |
| `ctx-clear` | Clear content area |

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `rayos.qemuPath` | `qemu-system-x86_64` | Path to QEMU |
| `rayos.targetArch` | `x86_64` | Target architecture |
| `rayos.enableSerial` | `true` | Enable serial output |
| `rayos.extraQemuArgs` | `""` | Additional QEMU args |

## Installation

### From Source

```bash
cd tools/vscode-rayos
npm install
npm run compile
```

Then press F5 in VS Code to launch the extension in debug mode.

### From VSIX

```bash
code --install-extension rayos-dev-0.1.0.vsix
```

## Development

```bash
# Install dependencies
npm install

# Compile
npm run compile

# Watch mode
npm run watch

# Package
npx vsce package
```

## Requirements

- VS Code 1.85.0 or later
- Rust toolchain with nightly
- QEMU (for running)

## License

MIT
