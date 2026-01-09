# RayOS Process Explorer

**Status**: In Development
**Target**: Q1 2026

---

## Overview

The Process Explorer is a graphical system monitoring tool for RayOS, inspired by Windows Process Explorer and htop. It provides real-time visibility into:

- **Ray Queue** - Pending compute tasks
- **System Components** - GPU Engine, LLM Engine, Conductor status
- **VMM Status** - Linux/Windows VM states
- **Memory Usage** - Heap, page tables, guest allocations
- **Interrupt Counters** - Timer, keyboard, mouse IRQs
- **Performance Metrics** - CPU cycles, VM exits

---

## Design Goals

1. **Native Integration** - Built into the kernel, not a separate app
2. **Real-time Updates** - Refreshes automatically (configurable rate)
3. **Interactive** - Click to expand details, kill tasks, adjust priorities
4. **Developer-Friendly** - Aids debugging during RayOS development

---

## UI Layout

```
┌─────────────────────────────────────────────────────────────┐
│ Process Explorer                                     [□][X] │
├─────────────────────────────────────────────────────────────┤
│ ┌─ System ─────────────────────────────────────────────────┐│
│ │ Uptime: 00:05:23    IRQs: Timer 19234  Kbd 47  Mouse 892 ││
│ │ Memory: 128MB used / 512MB total                         ││
│ └──────────────────────────────────────────────────────────┘│
│                                                             │
│ ┌─ Ray Queue ──────────────────────────────────────────────┐│
│ │ ID   Op       Priority  Status    Age                    ││
│ │ ──────────────────────────────────────────────────────── ││
│ │ 42   COMPUTE  High      Running   0.3s                   ││
│ │ 41   RENDER   Normal    Queued    1.2s                   ││
│ │ 40   STORAGE  Low       Queued    2.1s                   ││
│ │                                                          ││
│ │ Queue: 3 pending, 127 processed                          ││
│ └──────────────────────────────────────────────────────────┘│
│                                                             │
│ ┌─ Components ─────────────────────────────────────────────┐│
│ │ [●] System 1: GPU Engine     Running                     ││
│ │ [●] System 2: LLM Engine     Running                     ││
│ │ [●] Conductor                Active                      ││
│ │ [○] Linux VM                 Starting...                 ││
│ │ [○] Windows VM               Not configured              ││
│ └──────────────────────────────────────────────────────────┘│
│                                                             │
│ ┌─ Performance ────────────────────────────────────────────┐│
│ │ VMX Exits: 45,231   Avg Exit Time: 2.3µs                 ││
│ │ GPU Cmds:  1,892    Flush Rate: 60/s                     ││
│ └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation

### Data Sources

| Metric | Source |
|--------|--------|
| Uptime | `TIMER_TICKS` atomic counter |
| IRQ counts | `IRQ_*_COUNT` atomics |
| Ray Queue | `RAYQ_HEAD/TAIL`, `SYSTEM1_*` counters |
| Memory | Heap allocator stats, page table counts |
| VM status | `LINUX_DESKTOP_STATE`, VMM atomics |
| VMX stats | `EXTINTEXIT_COUNT`, exit timing |

### Window Type

New `WindowType::ProcessExplorer` for special handling:
- Auto-refresh content (every 500ms)
- No text input (read-only display)
- Larger default size (700x500)

### Rendering

Uses existing UI primitives:
- `draw_text()` for labels and values
- `fill_rect()` for section backgrounds
- Color coding: green=OK, yellow=warning, red=error

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `P` | Open Process Explorer |
| `R` | Refresh now |
| `Esc` | Close window |

---

## Future Enhancements

- **Task Kill** - Click to terminate a ray/process
- **Priority Adjustment** - Change task priorities
- **Graphs** - CPU/memory usage over time
- **Export** - Dump stats to serial/file
- **Filtering** - Search/filter by name or status

---

## Related

- [UI Framework](RAYOS_UI_FRAMEWORK.md)
- [System Architecture](SYSTEM_ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
