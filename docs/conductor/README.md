# RayOS Conductor

`conductor/` is host-side tooling for RayOS. The most important workflow today is the **AI bridge** used by the QEMU smoke tests.

## ai_bridge (host ↔ guest)
`src/bin/ai_bridge.rs` runs QEMU with `-serial stdio`, watches the guest serial output for:

```
RAYOS_INPUT:<id>:<text>
```

and replies back over the same serial channel:

```
AI:<id>:<chunk>
AI_END:<id>
```

This enables the bare-metal guest (`kernel-bare`) to request host-side “AI” and receive a correlated response.

## Build / run
The repo provides a canonical script that builds everything and runs the bridge:

```bash
./scripts/test-boot-ai-headless.sh
```

If you need to build manually:

```bash
cd conductor
cargo build --features ai,ai_ollama --bin ai_bridge
```

## Related components
- Guest protocol emitter/parser: `kernel-bare/src/main.rs`
- QEMU keystroke injection used by tests: `scripts/qemu-sendtext.py`
- Smoke tests:
	- `./scripts/test-boot-headless.sh` (boot markers)
	- `./scripts/test-boot-local-ai-headless.sh` (no host bridge)
	- `./scripts/test-boot-ai-headless.sh` (bridge)

## Features
- `ai_bridge` binary requires feature `ai` (see `Cargo.toml`).
- `ai_ollama` enables the optional HTTP backend integration.
