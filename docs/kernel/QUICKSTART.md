# RayOS Kernel - Quick Start Guide

## 🚀 Quick Commands

```bash
# Build the kernel
cargo build --release

# Run the kernel (demo mode)
cargo run --release

# Run tests
cargo test --all

# Check compilation
cargo check

# Generate documentation
cargo doc --open
```

## 📝 Basic Usage

### Creating a Kernel Instance

```rust
use rayos_kernel::RayKernelBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build with custom config
    let kernel = RayKernelBuilder::new()
        .with_dream_mode(true)
        .with_dream_timeout(300)  // 5 minutes
        .with_target_fps(60)
        .build()
        .await?;

    // Start the megakernel loop
    kernel.start().await?;

    Ok(())
}
```

### Submitting Tasks

```rust
// Natural language intent
kernel.process_input(
    Some("optimize the rendering pipeline"),
    None
).await?;

// Gaze-based input
kernel.process_input(
    None,
    Some((512.0, 384.0))  // x, y screen coordinates
).await?;

// Multimodal (combined)
kernel.process_input(
    Some("delete that file"),
    Some((100.0, 200.0))
).await?;
```

### Creating Custom Rays

```rust
use rayos_kernel::types::{LogicRay, Priority};
use glam::Vec3;

let ray = LogicRay::new(
    Vec3::new(0.0, 0.0, 0.0),     // Origin
    Vec3::new(1.0, 0.0, 0.0),     // Direction
    12345,                         // Task ID
    Priority::High,
    0,                             // Data pointer
    0,                             // Logic tree ID
);
```

### Monitoring System Metrics

```rust
let metrics = kernel.metrics();
println!("Queue depth: {}", metrics.queue_depth);
println!("Entropy: {:.2}", metrics.entropy);
println!("User present: {}", metrics.user_present);
```

## 🔧 Configuration Options

### KernelConfig Fields

```rust
pub struct KernelConfig {
    pub enable_dream_mode: bool,        // Enable autonomous optimization
    pub dream_timeout_secs: u64,        // Idle time before dream mode
    pub max_queue_size: usize,          // Maximum task queue size
    pub target_frame_time_us: u64,      // Target frame time (16666 = 60fps)
}
```

### Priority Levels

```rust
pub enum Priority {
    Dream = 0,        // Background optimization
    Low = 64,         // Low priority tasks
    Normal = 128,     // Normal user tasks
    High = 192,       // High priority
    Immediate = 255,  // Immediate user interaction
}
```

## 🧪 Testing

### Running Specific Tests

```bash
# Test System 1 (Reflex Engine)
cargo test system1

# Test System 2 (Cognitive Engine)
cargo test system2

# Test ray logic and BVH
cargo test ray_logic

# Test with output
cargo test -- --nocapture
```

### Example Test

```rust
#[tokio::test]
async fn test_kernel_initialization() {
    let kernel = RayKernelBuilder::new()
        .build()
        .await
        .expect("Kernel initialization failed");

    kernel.start().await.expect("Kernel start failed");

    // Kernel is now running
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    kernel.shutdown();
}
```

## 🔍 Debugging

### Enable Logging

```bash
# Info level (default)
RUST_LOG=info cargo run

# Debug level
RUST_LOG=debug cargo run

# Trace level (very verbose)
RUST_LOG=trace cargo run

# Module-specific
RUST_LOG=rayos_kernel::system1=debug cargo run
```

### Common Issues

**GPU Not Found**
- Ensure you have Vulkan/Metal/DX12 drivers installed
- Check `vulkaninfo` or equivalent for your platform

**Queue Overflow**
- Increase `max_queue_size` in config
- Check if megakernel loop is running

**High Latency**
- Reduce task submission rate
- Enable work stealing across multiple GPUs
- Lower `target_frame_time_us`

## 📊 Performance Tuning

### For High Throughput

```rust
let kernel = RayKernelBuilder::new()
    .with_max_queue_size(10_000_000)  // Large queue
    .with_target_fps(120)              // Higher frame rate
    .build()
    .await?;
```

### For Low Latency

```rust
let kernel = RayKernelBuilder::new()
    .with_max_queue_size(1_000)       // Small queue
    .with_target_fps(240)              // Very high frame rate
    .build()
    .await?;
```

### For Power Efficiency

```rust
let kernel = RayKernelBuilder::new()
    .with_target_fps(30)               // Lower frame rate
    .with_dream_mode(true)             // Optimize when idle
    .build()
    .await?;
```

## 🎯 Next Steps

1. **Explore the codebase**: Start with `src/lib.rs` and `src/types.rs`
2. **Run the demo**: `cargo run --release` and observe the output
3. **Modify configs**: Experiment with different kernel configurations
4. **Write tests**: Add new test cases for your use cases
5. **Read the design docs**: See `docs/ray-outline.md` and `docs/ray-summary.md`

## 📚 API Reference

Generate and browse the full API documentation:

```bash
cargo doc --no-deps --open
```

## 🤝 Development Workflow

```bash
# 1. Make changes
vim src/system1/megakernel.rs

# 2. Check compilation
cargo check

# 3. Run tests
cargo test

# 4. Fix warnings
cargo clippy

# 5. Format code
cargo fmt

# 6. Build release
cargo build --release

# 7. Run
cargo run --release
```

## 📖 Further Reading

- [The Bicameral Mind](https://en.wikipedia.org/wiki/Bicameral_mentality) - Inspiration for the architecture
- [Ray Tracing Gems](http://www.realtimerendering.com/raytracinggems/) - RT Core optimization
- [wgpu Documentation](https://wgpu.rs/) - GPU compute API

---

**Happy ray tracing! 🌟**
