# RayOS Application Framework Roadmap

**Document Version**: 1.0
**Created**: January 2026
**Status**: Planning

---

## Vision

Transform RayOS from an experimental OS into a **developer-friendly platform** with:

1. A complete native application framework (RayApps)
2. VS Code extension for streamlined development
3. Package management and distribution
4. Third-party app ecosystem

---

## Current State (January 2026)

### What Exists

| Component | Status | Notes |
|-----------|--------|-------|
| Window Manager | ✅ Implemented | Create, move, resize, focus |
| Compositor | ✅ Implemented | Z-order, damage tracking |
| Renderer | ✅ Implemented | Primitives, text, cursor |
| Input Handler | ✅ Implemented | Mouse, keyboard, text input |
| Shell | ✅ Implemented | Desktop integration |
| Content Routing | ✅ Implemented | Window-specific rendering |

### What's Missing for App Development

| Component | Status | Priority |
|-----------|--------|----------|
| Widget Library | ❌ Not started | P1 |
| Layout System | ❌ Not started | P1 |
| Event System | 🟡 Basic | P1 |
| App Manifest | ❌ Not started | P2 |
| Standalone Apps | ❌ Not started | P2 |
| Package Format | ❌ Not started | P3 |

---

## Roadmap

### Phase 1: Widget Foundation (Q1 2026)

**Goal**: Basic widget library for common UI patterns

#### Deliverables

1. **Core Widgets**
   - Button (with hover, pressed states)
   - Label (static text)
   - TextInput (single-line)
   - TextArea (multi-line)
   - Checkbox
   - Radio buttons

2. **Container Widgets**
   - Panel (grouping)
   - ScrollView (scrollable area)
   - TabView (tabbed interface)

3. **Layout Primitives**
   - VStack (vertical layout)
   - HStack (horizontal layout)
   - Grid (table layout)

#### Technical Design

```rust
// Widget trait
pub trait Widget {
    fn id(&self) -> WidgetId;
    fn bounds(&self) -> Rect;
    fn render(&self, ctx: &mut RenderContext);
    fn handle_event(&mut self, event: &Event) -> EventResult;
}

// Example usage
let button = Button::new("Click Me")
    .on_click(|| { /* handler */ });

let panel = VStack::new()
    .add(Label::new("Username:"))
    .add(TextInput::new().placeholder("Enter name"))
    .add(button);
```

---

### Phase 2: Application Model (Q2 2026)

**Goal**: Define how apps are structured and loaded

#### App Manifest Format

```toml
# rayapp.toml
[app]
name = "My Application"
version = "1.0.0"
author = "Developer Name"
description = "A sample RayOS application"

[window]
title = "My App"
width = 800
height = 600
resizable = true

[resources]
icon = "assets/icon.png"
fonts = ["assets/font.ttf"]

[permissions]
network = false
filesystem = "sandbox"
```

#### App Entry Point

```rust
// main.rs for a RayApp
#![no_std]
#![no_main]

use rayos_sdk::prelude::*;

#[rayapp::main]
fn main(app: &mut Application) {
    let window = app.create_window("My App", 800, 600);

    window.set_content(
        VStack::new()
            .add(Label::new("Hello, RayOS!"))
            .add(Button::new("Click").on_click(on_button_click))
    );
}

fn on_button_click() {
    println!("Button clicked!");
}
```

---

### Phase 3: Development Tooling (Q2-Q3 2026)

**Goal**: VS Code extension for RayOS development

#### Extension Features

| Feature | Description | Priority |
|---------|-------------|----------|
| Syntax Highlighting | RayApp manifest support | P1 |
| Build Tasks | cargo build integration | P1 |
| QEMU Launch | Run app in QEMU | P1 |
| Serial Monitor | View kernel logs | P1 |
| Project Templates | New RayApp wizard | P2 |
| IntelliSense | API completion | P2 |
| UI Preview | Live layout preview | P3 |

#### Extension Architecture

```
rayos-vscode/
├── package.json           # Extension manifest
├── src/
│   ├── extension.ts       # Extension entry
│   ├── build/             # Build task provider
│   ├── debug/             # Debug adapter
│   ├── language/          # Language support
│   └── preview/           # UI preview panel
└── resources/
    └── templates/         # Project templates
```

#### Sample Build Task

```json
{
    "label": "RayOS: Build App",
    "type": "rayos",
    "command": "build",
    "group": "build",
    "problemMatcher": "$rustc"
}
```

---

### Phase 4: SDK Stabilization (Q3 2026)

**Goal**: Stable SDK for app developers

#### SDK Crate Structure

```
rayos-sdk/
├── Cargo.toml
└── src/
    ├── lib.rs             # SDK entry point
    ├── app.rs             # Application lifecycle
    ├── window.rs          # Window management
    ├── widgets/           # Widget library
    │   ├── mod.rs
    │   ├── button.rs
    │   ├── label.rs
    │   └── ...
    ├── layout/            # Layout system
    ├── events/            # Event handling
    ├── graphics/          # Drawing API
    └── prelude.rs         # Common imports
```

#### Versioning Strategy

- SDK version tracks RayOS kernel compatibility
- Semantic versioning (MAJOR.MINOR.PATCH)
- Breaking changes only in major versions
- LTS releases with extended support

---

### Phase 5: Distribution (Q4 2026)

**Goal**: App packaging and distribution

#### Package Format (.rayapp)

```
myapp.rayapp
├── manifest.toml      # App metadata
├── app.wasm           # Compiled app (WASM)
├── assets/            # Resources
│   ├── icon.png
│   └── ...
└── signature          # Code signature
```

#### App Store (Future)

```
┌─────────────────────────────────────────────────────────────┐
│                   RayOS App Store                           │
├─────────────────────────────────────────────────────────────┤
│  Featured    Categories    Search                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  [App Icon] Terminal Pro        ★★★★☆                      │
│  A modern terminal emulator                                 │
│                                                             │
│  [App Icon] Notes               ★★★★★                      │
│  Simple note-taking app                                     │
│                                                             │
│  [App Icon] File Manager        ★★★★☆                      │
│  Browse and manage files                                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Technical Decisions

### Runtime Model

**Option A: Native ELF**
- Apps compiled to native ELF binaries
- Loaded directly by kernel
- Maximum performance
- Platform-specific compilation required

**Option B: WebAssembly** ✓ (Recommended)
- Apps compiled to WASM
- Sandboxed execution
- Cross-platform
- Slightly lower performance

**Decision**: Start with Option A (native) for kernel-internal apps, add WASM support for third-party apps.

### Rendering Architecture

**Option A: Immediate Mode**
- Render on every frame
- Simple implementation
- Higher CPU usage

**Option B: Retained Mode** ✓ (Recommended)
- Widget tree retained between frames
- Dirty tracking for efficient updates
- Complex but performant

**Decision**: Retained mode for production, immediate mode for prototyping.

---

## Milestones

| Milestone | Target Date | Deliverables |
|-----------|-------------|--------------|
| M1: Widgets Alpha | Feb 2026 | Button, Label, TextInput |
| M2: Layout System | Mar 2026 | VStack, HStack, Grid |
| M3: App Model | Apr 2026 | Manifest, entry point |
| M4: VS Code Basic | May 2026 | Build tasks, templates |
| M5: SDK Alpha | Jun 2026 | rayos-sdk crate |
| M6: VS Code Full | Aug 2026 | Debug, preview |
| M7: SDK Stable | Sep 2026 | 1.0 release |
| M8: Package Format | Nov 2026 | .rayapp format |
| M9: Distribution | Dec 2026 | App store MVP |

---

## Success Criteria

### Developer Experience

- [ ] New developer can create first app in < 30 minutes
- [ ] Build-run-debug cycle < 10 seconds
- [ ] API documentation with examples for all widgets
- [ ] VS Code extension in marketplace

### Technical Quality

- [ ] Apps sandboxed and cannot crash kernel
- [ ] Consistent 60 FPS UI performance
- [ ] Memory-safe APIs (Rust guarantees)
- [ ] Automated testing for SDK

### Ecosystem

- [ ] 10+ sample applications
- [ ] 3+ community-contributed apps
- [ ] Documentation website
- [ ] Developer forum/chat

---

## Open Questions

1. **How do apps access system services?**
   - System call interface?
   - Message passing?
   - Capability-based security?

2. **How do apps communicate with each other?**
   - Clipboard/drag-drop?
   - IPC channels?
   - Shared memory?

3. **How are app permissions managed?**
   - Manifest declarations?
   - Runtime prompts?
   - Per-app sandboxes?

4. **How do apps persist data?**
   - App-specific storage?
   - Shared databases?
   - Cloud sync?

---

## Contributing

We welcome contributions to the RayOS app framework! Areas where help is needed:

- Widget implementations
- Documentation and examples
- VS Code extension development
- Testing and bug reports

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

*This roadmap is a living document and will be updated as development progresses.*
