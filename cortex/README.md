# 👁️ RayOS Cortex - Phase 2: The Eyes

**The Sensory Processing Layer of RayOS**

Cortex implements the "Vision Pathway" from the RayOS architecture, connecting gaze tracking, object recognition, and LLM-based intent interpretation to create a natural, eyes-first interface.

---

## 🎯 What is Cortex?

Cortex is the sensory input processor for RayOS. It:

- 📹 **Captures video** from your webcam
- 👀 **Tracks your gaze** to understand what you're looking at
- 🔍 **Recognizes objects** in your visual field
- 🧠 **Interprets intent** using a local LLM
- 📡 **Communicates** with the kernel's System 2 (Cognitive Engine)

Think of it as the "eyes and attention" of the operating system.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Cortex Main Loop                     │
│  (Orchestrates all sensory processing at ~60Hz)        │
└────────────────┬────────────────────────────────────────┘
                 │
        ┌────────┴────────┐
        │                 │
  ┌─────▼──────┐   ┌─────▼──────┐
  │   Vision   │   │    LLM     │
  │  Pathway   │   │ Connector  │
  └─────┬──────┘   └─────┬──────┘
        │                 │
  ┌─────▼──────┐          │
  │   Gaze     │          │
  │  Tracker   │          │
  └─────┬──────┘          │
        │                 │
  ┌─────▼──────┐   ┌─────▼──────┐
  │  Object    │   │   Intent   │
  │ Recognizer │   │  Parser    │
  └─────┬──────┘   └─────┬──────┘
        │                 │
        └────────┬────────┘
                 │
        ┌────────▼──────────┐
        │ Audio-Visual      │
        │     Fusion        │
        └───────────────────┘
```

---

## 📦 Modules

### 1. **Vision Pathway** (`src/vision/`)

The core visual processing system.

- **Gaze Tracker** (`gaze_tracker.rs`)
  - Uses OpenCV Haar Cascades for face/eye detection
  - Calculates screen coordinates of user's gaze
  - Outputs normalized (x, y) coordinates with confidence scores

- **Object Recognizer** (`object_recognizer.rs`)
  - Stub for ML-based object detection (YOLO, MobileNet, etc.)
  - Identifies objects in the visual field
  - Returns bounding boxes and confidence scores

### 2. **Audio-Visual Fusion** (`fusion.rs`)

Combines multiple sensory modalities to understand context.

- Maintains gaze history (temporal context)
- Buffers recent audio transcripts
- Resolves deictic references ("that", "this", "it") by correlating gaze with objects
- Detects fixation (prolonged gaze on specific areas)

### 3. **LLM Connector** (`llm.rs`)

Interprets fused context to determine user intent.

- Currently uses heuristic-based classification
- Designed to integrate with Candle/Llama.cpp for full LLM inference
- Classifies intents: Select, Move, Delete, Create, Break, Idle

---

## 🚀 Quick Start

### Prerequisites

1. **Rust** (1.70+)
2. **OpenCV** (4.x)
   ```bash
   # Ubuntu/Debian
   sudo apt install libopencv-dev clang libclang-dev

   # macOS
   brew install opencv
   ```
3. **Webcam** (built-in or USB)

### Build

```bash
cd cortex
cargo build --release
```

### Run

```bash
cargo run --release
```

You should see:

```
═══════════════════════════════════════
  RayOS Cortex - Phase 2: The Eyes
═══════════════════════════════════════
[INFO] Initializing RayOS Cortex...
[INFO] Initializing Vision Pathway...
[INFO] Cortex initialized successfully!
[INFO] Starting main processing loop...
Press Ctrl+C to exit
───────────────────────────────────────
```

---

## 📊 Current Status

### ✅ Implemented

- Core architecture and event loop
- Camera capture via OpenCV
- Basic gaze estimation from face position
- Object detection stubs
- Audio-visual fusion logic
- Heuristic-based intent classification
- Graceful shutdown

### 🚧 In Progress

- Hardware eye tracking integration (Tobii, etc.)
- Real ML-based object recognition (YOLO/MobileNet)
- Full LLM integration with Candle
- Communication protocol with kernel

### 📋 Planned (Phase 3+)

- Audio input via microphone
- Speech-to-text transcription (Whisper)
- Vector store integration for semantic memory
- GPU-accelerated inference
- Multi-monitor gaze tracking

---

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

Example test:

```rust
#[tokio::test]
async fn test_break_detection() {
    let llm = LLMConnector::new().await.unwrap();
    let context = /* ... gaze + cup object ... */;

    let intent = llm.process_context(&context).await.unwrap();
    assert!(matches!(intent, Some(Intent::Break)));
}
```

---

## 🎓 Key Concepts

### Gaze as First-Class Input

Unlike traditional UIs that rely on mouse/keyboard, RayOS treats **gaze** as the primary input modality. The cursor follows your eyes, not your hand.

### Deictic Reference Resolution

When you say "Delete that" while looking at a file, Cortex:
1. Captures your gaze coordinates
2. Identifies objects at that location
3. Resolves "that" → specific file
4. Sends `Delete(file_id)` intent to kernel

### Multimodal Fusion

Vision and audio alone are ambiguous:
- "Delete" (audio) → Delete what?
- Looking at file (vision) → Just observing?

**Together:** "Delete" + looking at file = Clear intent

---

## 🔧 Configuration

Create `cortex.toml` (optional):

```toml
[camera]
device_index = 0

[vision]
target_fps = 60
enable_debug = false

[llm]
model_path = "models/llama-7b.gguf"
device = "cuda"  # or "cpu"
```

---

## 🐛 Troubleshooting

### "Failed to open camera"

- Check camera permissions
- Verify camera index (try 1, 2 if 0 fails)
- Test with: `ffplay /dev/video0` (Linux)

### "Could not load face cascade"

- OpenCV not installed properly
- Haar cascade files missing
- Update paths in `gaze_tracker.rs`

### High CPU usage

- Lower `target_fps` in config
- Disable debug mode
- Use GPU for ML inference

---

## 🤝 Integration with Kernel

Cortex communicates with the kernel's **System 2 (Cognitive Engine)** via:

1. Shared memory channels
2. Intent messages serialized as `TaskStruct`
3. Priority-based task queue

Example flow:

```
User looks at file.txt + says "Open this"
    ↓
Cortex: Detect gaze (0.3, 0.5) + audio "Open this"
    ↓
Fusion: Resolve "this" → file.txt
    ↓
LLM: Classify → Intent::Select { target: "file.txt" }
    ↓
Send to Kernel System 2
    ↓
Kernel dispatches 10,000 "File Open" rays to GPU
```

---

## 📚 API Example

```rust
use rayos_cortex::Cortex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cortex = Cortex::new().await?;

    cortex.run().await?;

    Ok(())
}
```

---

## 🌟 Why This Matters

Traditional operating systems are **reactive** (you click, they respond).

RayOS is **perceptive** (it sees what you see, understands what you mean).

Cortex is the first step toward an OS that doesn't just execute commands—it **anticipates** them.

---

## 📖 Further Reading

- [RayOS Architecture Overview](../kernel/docs/ray-outline.md)
- [Phase 1: The Skeleton](../kernel/BUILD_SUMMARY.md)
- [Phase 3: The Memory](../kernel/README.md#phase-3)

---

## 🎯 Next Steps (Phase 3)

After Phase 2 (The Eyes), we'll build:

- **Vector Store**: Semantic file system
- **Epiphany Buffer**: Dream-state ideation
- **HNSW Indexer**: Similarity search in VRAM

Stay tuned! 👁️

---

**Built with 🧠 for RayOS**
