# Contributing to RayOS

Thank you for your interest in contributing to RayOS! This document provides guidelines for contributing to the project.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Setup](#development-setup)
3. [Code Style](#code-style)
4. [Pull Request Process](#pull-request-process)
5. [Areas for Contribution](#areas-for-contribution)

---

## Getting Started

### Prerequisites

- **Rust** (nightly toolchain, managed via rustup)
- **QEMU** with OVMF for testing
- **Git** for version control
- **Linux** recommended (WSL2 on Windows works)

### Clone and Build

```bash
git clone https://github.com/your-org/RayOS.git
cd RayOS

# Build everything
./scripts/build-iso.sh

# Run tests
./scripts/test-ui-shell-headless.sh
```

---

## Development Setup

### Directory Structure

```
crates/
├── kernel-bare/        # Main kernel (most contributions here)
│   └── src/
│       ├── main.rs     # Kernel entry point
│       └── ui/         # UI framework
├── bootloader/         # UEFI bootloader
├── volume/             # Storage management
└── cortex/             # AI components

scripts/                # Build and test scripts
docs/                   # Documentation
```

### Building the Kernel

```bash
cd crates/kernel-bare
cargo build --features ui_shell,serial_debug --target x86_64-unknown-none --release
```

### Testing Changes

```bash
# Quick headless test
./scripts/test-ui-shell-headless.sh

# Interactive test
./scripts/run-ui-shell.sh
```

---

## Code Style

### Rust Guidelines

- Follow standard Rust formatting (`cargo fmt`)
- Use `#[allow(...)]` sparingly and document why
- Prefer explicit error handling over `.unwrap()`
- Document public APIs with doc comments

### Naming Conventions

```rust
// Modules: snake_case
mod window_manager;

// Types: PascalCase
pub struct WindowManager { ... }
pub enum WindowType { ... }

// Functions: snake_case
pub fn create_window(...) { ... }

// Constants: SCREAMING_SNAKE_CASE
pub const MAX_WINDOWS: usize = 64;

// Statics: SCREAMING_SNAKE_CASE with AtomicXxx for mutability
static WINDOW_COUNT: AtomicU32 = AtomicU32::new(0);
```

### Documentation

```rust
/// Creates a new window with the specified parameters.
///
/// # Arguments
///
/// * `title` - Window title (max 64 bytes)
/// * `x`, `y` - Initial position
/// * `width`, `height` - Initial dimensions
/// * `window_type` - Type of window (Normal, Dialog, etc.)
///
/// # Returns
///
/// The window ID, or `None` if creation failed.
///
/// # Example
///
/// ```rust
/// let id = create_window(b"My Window", 100, 100, 400, 300, WindowType::Normal);
/// ```
pub fn create_window(...) -> Option<WindowId> { ... }
```

### Serial Debug Output

For debugging, use deterministic markers:

```rust
#[cfg(feature = "serial_debug")]
{
    crate::serial_write_str("RAYOS_MY_FEATURE:");
    crate::serial_write_str("description\n");
}
```

Marker format: `RAYOS_COMPONENT_ACTION:value`

---

## Pull Request Process

### Before Submitting

1. **Test your changes**
   ```bash
   ./scripts/test-ui-shell-headless.sh
   # Ensure PASS output
   ```

2. **Format code**
   ```bash
   cargo fmt --check
   ```

3. **Check for errors**
   ```bash
   cargo build --features ui_shell,serial_debug --target x86_64-unknown-none --release 2>&1 | grep "^error"
   # Should produce no output
   ```

### PR Requirements

- Clear description of changes
- Reference any related issues
- Include test results
- Screenshots for UI changes

### Review Process

1. Submit PR against `main` branch
2. Automated tests run
3. Code review by maintainers
4. Address feedback
5. Merge when approved

---

## Areas for Contribution

### High Priority

| Area | Description | Difficulty |
|------|-------------|------------|
| Widgets | Button, Checkbox, etc. | Medium |
| Layout | VStack, HStack, Grid | Medium |
| Documentation | Examples, tutorials | Easy |
| Tests | Headless test coverage | Easy |

### Medium Priority

| Area | Description | Difficulty |
|------|-------------|------------|
| Themes | Color schemes, styling | Medium |
| Fonts | TrueType font support | Hard |
| Animations | Window transitions | Hard |
| Accessibility | Keyboard navigation | Medium |

### Future

| Area | Description | Difficulty |
|------|-------------|------------|
| VS Code Extension | Dev tooling | Hard |
| WASM Runtime | App sandboxing | Very Hard |
| App Store | Distribution | Hard |

---

## Communication

- **Issues**: Bug reports, feature requests
- **Discussions**: Design questions, ideas
- **Pull Requests**: Code contributions

---

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT).

---

*Thank you for helping make RayOS better!*
