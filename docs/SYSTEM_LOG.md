# RayOS System Log

**Status**: Planned
**Target**: Q1 2026
**Priority**: High

---

## Overview

A robust in-kernel event journal for RayOS that allows troubleshooting system behavior without relying on external serial logging. The System Log captures kernel events, subsystem state changes, and debug information in a ring buffer that can be viewed in a native UI window.

## Motivation

Current debugging relies on serial output to host, which:
- Requires external tooling to capture
- Is unavailable in production deployments
- Cannot be viewed within RayOS itself
- Is difficult to filter or search

The System Log provides:
- **Self-contained diagnostics** - View logs without leaving RayOS
- **Persistent ring buffer** - Capture events even before UI is available
- **Severity levels** - Filter by importance
- **Subsystem tags** - Focus on specific components
- **Timestamps** - Correlate events with uptime
- **Searchable** - Find specific events quickly

---

## Architecture

### Ring Buffer Design

```
┌─────────────────────────────────────────────────────┐
│                  SYSTEM_LOG_BUFFER                  │
│  [Entry 0] [Entry 1] [Entry 2] ... [Entry N-1]     │
│       ↑                                    ↑        │
│    read_idx                            write_idx    │
└─────────────────────────────────────────────────────┘

Entry Layout (64 bytes):
┌──────────┬──────────┬──────────┬───────────────────┐
│ timestamp│ severity │ subsystem│ message (52 bytes)│
│  8 bytes │  1 byte  │  1 byte  │                   │
└──────────┴──────────┴──────────┴───────────────────┘
```

### Constants

```rust
const LOG_BUFFER_ENTRIES: usize = 1024;  // 64KB total
const LOG_ENTRY_SIZE: usize = 64;
const LOG_MESSAGE_MAX: usize = 52;
```

### Severity Levels

| Level | Value | Color | Use Case |
|-------|-------|-------|----------|
| TRACE | 0 | Gray | Verbose debugging |
| DEBUG | 1 | Cyan | Development info |
| INFO  | 2 | White | Normal events |
| WARN  | 3 | Yellow | Potential issues |
| ERROR | 4 | Red | Failures |
| FATAL | 5 | Magenta | Critical failures |

### Subsystem Tags

| Tag | Value | Component |
|-----|-------|-----------|
| KERNEL | 0x01 | Core kernel |
| MEMORY | 0x02 | Heap/page allocator |
| IRQ | 0x03 | Interrupt handlers |
| TIMER | 0x04 | Timer subsystem |
| KEYBOARD | 0x05 | PS/2 keyboard |
| MOUSE | 0x06 | PS/2 mouse |
| UI | 0x10 | Window manager, compositor |
| INPUT | 0x11 | Input handling |
| RENDER | 0x12 | Rendering |
| VMM | 0x20 | Hypervisor |
| GUEST | 0x21 | Linux/Windows guest |
| VIRTIO | 0x22 | virtio devices |
| RAY | 0x30 | Ray queue system |
| CONDUCTOR | 0x31 | Conductor orchestrator |
| AI | 0x40 | AI/LLM subsystem |
| SECURITY | 0x50 | Security/policy |
| APP | 0x60 | RayApp framework |

---

## API

### Core Functions

```rust
/// Log an event to the system log.
pub fn syslog(severity: u8, subsystem: u8, message: &[u8]);

/// Convenience macros with severity
macro_rules! log_trace { ... }
macro_rules! log_debug { ... }
macro_rules! log_info { ... }
macro_rules! log_warn { ... }
macro_rules! log_error { ... }

/// Get log entries (returns iterator over visible entries)
pub fn syslog_iter() -> impl Iterator<Item = &LogEntry>;

/// Get entry count
pub fn syslog_count() -> usize;

/// Clear the log
pub fn syslog_clear();
```

### Usage Examples

```rust
// Basic logging
syslog(SEVERITY_INFO, SUBSYSTEM_KERNEL, b"System initialized");

// Using macros
log_info!(SUBSYSTEM_UI, "Window {} created", window_id);
log_warn!(SUBSYSTEM_MEMORY, "Heap at 90% capacity");
log_error!(SUBSYSTEM_VMM, "VMX exit handler failed");
```

---

## UI Design

### System Log Window

```
┌─────────────────────────────────────────────────────┐
│ [×] System Log                              [─][□] │
├─────────────────────────────────────────────────────┤
│ Filter: [All ▼] [_____________] [Clear] [Pause]    │
├─────────────────────────────────────────────────────┤
│ 00:00:01.234 INFO  KERNEL   System initialized     │
│ 00:00:01.235 DEBUG MEMORY   Heap: 16MB available   │
│ 00:00:01.456 INFO  UI       Window manager started │
│ 00:00:02.100 INFO  KEYBOARD PS/2 keyboard init     │
│ 00:00:02.150 INFO  MOUSE    PS/2 mouse init        │
│ 00:00:05.000 WARN  GUEST    No scanout yet         │
│ 00:00:10.234 INFO  VMM      VMX exits: 10000       │
│ ▼ (auto-scroll)                                    │
├─────────────────────────────────────────────────────┤
│ 1024 entries │ Showing: 1024 │ Filter: none        │
└─────────────────────────────────────────────────────┘
```

### Features

- **Auto-scroll** - Follows new entries (toggle with Pause)
- **Filter dropdown** - By severity or subsystem
- **Search box** - Text search in messages
- **Color coding** - By severity level
- **Timestamp format** - HH:MM:SS.mmm relative to boot
- **Export** - Copy to clipboard (future)

### Commands

```
show system log       - Open the System Log window
show log             - Shortcut
syslog               - Shortcut
```

---

## Implementation Plan

### Phase 1: Core Infrastructure

1. Create `src/syslog.rs` module
2. Implement ring buffer with atomic indices
3. Add `syslog()` function and macros
4. Add logging to existing serial_write_str calls

### Phase 2: Integration

1. Instrument key subsystems:
   - Kernel init sequence
   - Memory allocator
   - IRQ handlers (throttled)
   - Window manager events
   - Input events (throttled)
   - VMM exit handlers (sampled)

### Phase 3: UI

1. Add `render_system_log()` in content.rs
2. Add `show_system_log()` in shell.rs
3. Add command parsing in content.rs and main.rs
4. Implement filtering and search

### Phase 4: Polish

1. Severity-based coloring
2. Subsystem filtering
3. Auto-scroll toggle
4. Entry count display

---

## Performance Considerations

### Log Entry Cost

- **Fast path**: ~50 cycles for ring buffer write
- **No allocation**: Fixed-size entries
- **Lock-free**: Atomic write index, sequential reads
- **Throttling**: Rate-limit high-frequency events

### Memory Usage

- **Fixed buffer**: 64KB (1024 entries × 64 bytes)
- **No growth**: Ring buffer wraps
- **Static allocation**: No heap pressure

### Throttling Strategy

```rust
static LAST_IRQ_LOG_TICK: AtomicU64 = AtomicU64::new(0);

fn log_irq_event(msg: &[u8]) {
    let now = TIMER_TICKS.load(Ordering::Relaxed);
    let last = LAST_IRQ_LOG_TICK.load(Ordering::Relaxed);

    // Log at most once per 100 ticks (~1 second)
    if now.saturating_sub(last) >= 100 {
        LAST_IRQ_LOG_TICK.store(now, Ordering::Relaxed);
        syslog(SEVERITY_DEBUG, SUBSYSTEM_IRQ, msg);
    }
}
```

---

## Data Sources

| Event | Subsystem | Severity | Frequency |
|-------|-----------|----------|-----------|
| Boot stages | KERNEL | INFO | Once |
| Heap alloc | MEMORY | DEBUG | Throttled |
| Page fault | MEMORY | WARN | Each |
| Timer tick | TIMER | TRACE | Sampled |
| Key press | KEYBOARD | DEBUG | Each |
| Mouse move | MOUSE | TRACE | Sampled |
| Window create | UI | INFO | Each |
| Window focus | UI | DEBUG | Each |
| Compositor frame | RENDER | TRACE | Sampled |
| VMX exit | VMM | TRACE | Sampled |
| Guest scanout | GUEST | INFO | Each |
| Ray enqueue | RAY | DEBUG | Throttled |
| Ray process | RAY | DEBUG | Throttled |
| AI prompt | AI | INFO | Each |
| AI response | AI | INFO | Each |

---

## Testing

### Unit Tests

- Ring buffer wrap-around
- Entry format correctness
- Filter matching
- Timestamp formatting

### Integration Tests

- Log survival across subsystem init
- UI rendering with 1000+ entries
- Performance under high log rate
- Memory stability (no leaks)

---

## Future Enhancements

- **Persistent log**: Save to disk on shutdown
- **Remote access**: View logs over network
- **Structured data**: JSON-like fields
- **Log levels config**: Runtime severity threshold
- **Export**: Copy buffer to file/clipboard
- **Crash log**: Preserve across panic/reboot

---

## Dependencies

- `TIMER_TICKS` - For timestamps
- UI framework - For log viewer window
- Atomic operations - For lock-free buffer

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/syslog.rs` | Create - Core implementation |
| `src/main.rs` | Modify - Add syslog calls |
| `src/ui/content.rs` | Modify - Add render_system_log |
| `src/ui/shell.rs` | Modify - Add show_system_log |
| `docs/SYSTEM_LOG.md` | Create - This document |
| `docs/ROADMAP.md` | Modify - Add to planned features |
