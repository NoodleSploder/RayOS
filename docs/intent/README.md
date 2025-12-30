# RayOS Intent - Phase 5: Natural Language Understanding

**The Cognitive Engine / System 2**

Natural language understanding and intent parsing for RayOS. Translates user input (speech + gaze + context) into executable commands with resource allocation policies.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              Intent Engine (Phase 5)                 │
├─────────────────────────────────────────────────────┤
│                                                       │
│  ┌──────────────┐         ┌──────────────┐          │
│  │ Intent Parser │────────▶│ LLM Connector│          │
│  │              │         │   (optional)  │          │
│  │ NL→Command   │         │  Candle GPU  │          │
│  └──────┬───────┘         └──────────────┘          │
│         │                                             │
│         │ Intent    ┌──────────────────┐             │
│         └──────────▶│ Policy Arbiter   │             │
│                     │                  │             │
│                     │ Resource         │             │
│  ┌──────────────┐  │ Allocation       │             │
│  │Context       │  └────────┬─────────┘             │
│  │Manager       │           │                        │
│  │              │           │ Policy                 │
│  │Audio-Visual  │           ▼                        │
│  │Fusion        │     ┌──────────────┐              │
│  └──────────────┘     │ Conductor    │              │
│                       │ (Phase 4)    │              │
│  Gaze + Audio         └──────────────┘              │
│  + Visual Objects                                    │
└─────────────────────────────────────────────────────┘
```

## Components

### 1. Intent Parser
Translates natural language to structured commands:
- Pattern-based parsing (heuristic mode)
- Optional LLM parsing (neural mode)
- Deictic reference resolution ("that", "this", "it")
- Context-aware command interpretation

### 2. Policy Arbiter
Dynamic resource allocation:
- Priority-based scheduling (Realtime → Idle)
- System load monitoring
- Resource constraints (CPU, memory, GPU, time)
- Policy enforcement

### 3. Context Manager
Sensor fusion and context tracking:
- Gaze history (eye tracking)
- Audio buffer (speech transcription)
- Visual object detection
- Filesystem awareness
- 30-second rolling context window

### 4. LLM Connector
Optional neural language understanding:
- Candle-based BERT/GPT models
- GPU acceleration (CUDA/Metal)
- Fallback to heuristic mode
- Semantic similarity

## Command Types

### Create
```rust
Command::Create {
    object_type: "file",
    properties: { "name": "test.rs" }
}
```

### Modify
```rust
Command::Modify {
    target: Target::Deictic { gaze_position: (100, 200) },
    operation: Operation::Rename { new_name: "new.rs" }
}
```

### Query
```rust
Command::Query {
    query: "find all rust files",
    filters: [Filter { field: "extension", operator: Equals, value: "rs" }]
}
```

### Execute
```rust
Command::Execute {
    action: "cargo",
    args: ["build", "--release"]
}
```

## Usage

### Basic Example
```rust
use rayos_intent::{IntentEngine, IntentConfig};

let config = IntentConfig::default();
let engine = IntentEngine::new(config);

// Parse user command
let result = engine.parse("find all rust files");

if result.needs_clarification {
    println!("Please clarify: {:?}", result.intent.command);
} else {
    // Execute intent
    let status = engine.execute(result.intent)?;
    println!("Status: {:?}", status);
}
```

### With Context
```rust
// Update gaze from eye tracker
engine.update_gaze((100.0, 200.0), Some("file.rs".to_string()));

// Update audio from microphone
engine.update_audio("delete that".to_string(), audio_samples);

// Parse with context
let result = engine.parse("delete that");
// Resolves "that" → "file.rs" via gaze
```

### REPL Mode
```bash
cargo run --bin rayos-intent -- repl
```

```
> find rust files
  Command: Query { query: "rust files", filters: [...] }
  Confidence: 0.85
  ✓ Ready to execute

> gaze
Gaze updated to (100, 200) on test.rs

> delete that
  Command: Delete { target: Deictic { object_id: "test.rs" } }
  Confidence: 0.78
  ✓ Ready to execute
```

## Priority Levels

| Priority | Use Case | Max Latency | Resource % |
|----------|----------|-------------|------------|
| **Realtime** | UI navigation | 16ms | 150% |
| **Interactive** | User commands | 100ms | 120% |
| **Normal** | Background work | 1s | 100% |
| **Low** | Deferred tasks | 5s | 70% |
| **Idle** | Dream mode | 30s | 30% |

## Features

### Default (Heuristic Mode)
```toml
[dependencies]
rayos-intent = "0.1.0"
```

### With LLM Support
```toml
[dependencies]
rayos-intent = { version = "0.1.0", features = ["llm"] }
```

### Full (All Features)
```toml
[dependencies]
rayos-intent = { version = "0.1.0", features = ["full"] }
```

## Configuration

```rust
let config = IntentConfigBuilder::new()
    .enable_llm(true)
    .llm_model_path("models/bert-base.safetensors".into())
    .confidence_threshold(0.8)
    .enable_fusion(true)
    .enforce_policy(true)
    .build();

let mut engine = IntentEngine::new(config);
engine.initialize()?;  // Load LLM
```

## Integration

### With Conductor (Phase 4)
```rust
// Parse intent
let result = engine.parse("optimize system");

// Get policy
let policy = engine.allocate_resources(&result.intent);

// Convert to Conductor task
let task = Task {
    id: TaskId::new(),
    priority: match policy.priority {
        Priority::Realtime => conductor::Priority::Critical,
        Priority::Interactive => conductor::Priority::High,
        _ => conductor::Priority::Normal,
    },
    payload: TaskPayload::Optimize,
};

// Submit to Conductor
conductor.submit_task(task)?;
```

### With Cortex (Phase 2)
```rust
// Receive gaze from Cortex
cortex.on_gaze(|position, object| {
    engine.update_gaze(position, object);
});

// Receive audio transcription
cortex.on_audio(|transcript, samples| {
    engine.update_audio(transcript, samples);
});
```

## CLI Commands

```bash
# Parse single command
rayos-intent parse "find all rust files"

# Interactive REPL
rayos-intent repl

# Run test suite
rayos-intent test

# Show engine info
rayos-intent info
```

## Performance

- **Heuristic parsing**: <1ms
- **LLM parsing (CPU)**: 10-50ms
- **LLM parsing (GPU)**: 5-15ms
- **Context fusion**: <1ms
- **Policy allocation**: <1ms

## Testing

```bash
# Run unit tests
cargo test

# Run with output
cargo test -- --nocapture

# Specific module
cargo test intent_parser
```

## Examples

See [examples/](examples/) for:
- Simple command parsing
- Deictic resolution demo
- LLM integration
- Full pipeline example

## Design Philosophy

**System 2 Thinking**: Intent represents deliberate, conscious reasoning about user goals. Unlike System 1 (Cortex's reflexive processing), Intent:

1. **Thinks before acting** - Parses, validates, allocates resources
2. **Handles ambiguity** - Requests clarification when uncertain
3. **Considers context** - Fuses audio, visual, and historical data
4. **Enforces safety** - Sandboxes risky operations, ensures reversibility
5. **Optimizes globally** - Balances priorities across the entire system

## Future Work

- [ ] Full LLM integration with prompt engineering
- [ ] Multi-turn conversation handling
- [ ] Learning from corrections
- [ ] Custom DSL for power users
- [ ] Integration with Volume (Phase 3) for semantic search
- [ ] Voice activity detection (VAD)
- [ ] Multi-language support

## License

MIT

## See Also

- [Phase 1: Cortex](../cortex/) - Sensory input processing
- [Phase 4: Conductor](../conductor/) - Task orchestration
- [Phase 3: Volume](../volume/) - Semantic file system
- [Ray Outline](../docs/ray-outline.md) - Full system architecture
