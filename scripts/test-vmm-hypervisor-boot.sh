#!/bin/bash
# Headless-ish smoke test for the in-kernel hypervisor skeleton.
#
# What it validates:
# - kernel boots with `vmm_hypervisor` enabled
# - VMX bring-up path runs and emits deterministic serial markers
# - If VM-entry succeeds, we should see at least one VM-exit marker
#
# Notes:
# - This script prefers KVM if /dev/kvm is present.
# - Without KVM, it will try TCG with a VMX-capable CPU model; VMX instructions
#   may still not be usable depending on QEMU build.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-hypervisor-boot.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-hypervisor-boot.norm.log"

# Avoid mixing output from prior runs.
: > "$SERIAL_LOG"

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

# Ensure the feature is enabled (append if user provided other features).
RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
if [ -z "$RAYOS_KERNEL_FEATURES" ]; then
  RAYOS_KERNEL_FEATURES="vmm_hypervisor,vmm_hypervisor_smoke,vmm_virtio_console"
elif ! echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",vmm_hypervisor,"; then
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES},vmm_hypervisor"
fi

if ! echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",vmm_hypervisor_smoke,"; then
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES},vmm_hypervisor_smoke"
fi
export RAYOS_KERNEL_FEATURES

# Default to headless for CI/determinism, but allow `HEADLESS=0` to show a QEMU window.
export HEADLESS="${HEADLESS:-1}"

# Keep this a finite smoke test, but avoid using `timeout(1)` so QEMU doesn't
# get killed with SIGTERM (noisy logs). We'll quit via monitor instead.
TEST_TIMEOUT_SECS="${TEST_TIMEOUT_SECS:-20}"

# Make sure we capture a fresh serial.
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  # Best-effort: expose VMX in the emulated CPU.
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

echo "Running hypervisor boot smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  TEST_TIMEOUT_SECS=$TEST_TIMEOUT_SECS" >&2
echo "  QEMU_EXTRA_ARGS=$QEMU_EXTRA_ARGS" >&2

echo "Building kernel-bare with vmm_hypervisor enabled..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

if [ "$HEADLESS" = "0" ]; then
  BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" || true
else
  MON_SOCK_MAIN="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-headless.sock"
  run_headless_boot_until "$SERIAL_LOG" "$MON_SOCK_MAIN" "$TEST_TIMEOUT_SECS" \
    'RAYOS_VMM:VMX:UNSUPPORTED' \
    'RAYOS_VMM:VMX:INIT_BEGIN' \
    'RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT|RAYOS_VMM:VMX:SUPPORTED'
fi

# Normalize CRLF.
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

NEED1="RAYOS_VMM:VMX:INIT_BEGIN"
NEED2="RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT"
NEED2_ALT="RAYOS_VMM:VMX:SUPPORTED"
NEED3="RAYOS_LINUX_DESKTOP_PRESENTED"
NEED4="RAYOS_LINUX_DESKTOP_FIRST_FRAME"

if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$SERIAL_NORM"; then
  echo "SKIP: VMX unsupported in this QEMU configuration" >&2
  exit 0
fi

if grep -F -a -q "$NEED1" "$SERIAL_NORM" && (grep -F -a -q "$NEED2" "$SERIAL_NORM" || grep -F -a -q "$NEED2_ALT" "$SERIAL_NORM"); then
  echo "PASS: hypervisor init path executed (markers present)" >&2
  if grep -F -a -q "RAYOS_VMM:VMX:VMEXIT" "$SERIAL_NORM"; then
    echo "PASS: observed VM-exit marker" >&2
  else
    echo "NOTE: no VM-exit marker observed (likely VM-entry failed)" >&2
    if grep -F -a -q "RAYOS_VMM:VMX:VM_INSTR_ERR=" "$SERIAL_NORM"; then
      echo "NOTE: VM-instruction error printed (see serial log)" >&2
    fi
  fi

  # Check virtio-gpu selftest markers when present in this build.
  if grep -F -a -q "$NEED3" "$SERIAL_NORM" && grep -F -a -q "$NEED4" "$SERIAL_NORM"; then
    echo "PASS: virtio-gpu selftest published scanout + first-frame markers" >&2
  else
    echo "NOTE: virtio-gpu selftest markers missing; check build features" >&2
    # Non-fatal; the hypervisor init still counts as success for general CI.
  fi

  # Optional: check virtio-console dispatch marker when the feature is present.
  if echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",vmm_virtio_console,"; then
    if grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:CHAIN_HANDLED" "$SERIAL_NORM" || grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:ENABLED" "$SERIAL_NORM" || grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:COMPILED" "$SERIAL_NORM" || grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:RECV" "$SERIAL_NORM" || grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST:INVOKE" "$SERIAL_NORM"; then
      echo "PASS: virtio-console present (enabled, selftest, or dispatch observed)" >&2
    else
      echo "NOTE: virtio-console not observed; check build features" >&2
    fi

    # Optional: deterministic guest-driven console test: build a guest blob that writes a console message
    SERIAL_LOG_CONSOLE="$WORK_DIR/serial-vmm-hypervisor-boot.console.log"
    SERIAL_NORM_CONSOLE="$WORK_DIR/serial-vmm-hypervisor-boot.console.norm.log"
    : > "$SERIAL_LOG_CONSOLE"
    echo "Running guest-driven virtio-console test (RAYOS_GUEST_CONSOLE_ENABLED=1)" >&2
    pushd "$ROOT_DIR/scripts" >/dev/null
    # Build the generator as a small executable and run it to write the guest blob
    rustc generate_guest_driver.rs -O -o generate_guest_driver >/dev/null 2>&1 || true
    RAYOS_GUEST_NET_ENABLED=0 RAYOS_GUEST_CONSOLE_ENABLED=1 ./generate_guest_driver >/dev/null 2>&1 || true
    rm -f generate_guest_driver || true
    popd >/dev/null

    pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
    RUSTC="$(rustup which rustc)" cargo build \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none \
      --features "${RAYOS_KERNEL_FEATURES}" \
      >/dev/null
    popd >/dev/null

    MON_SOCK_CONSOLE="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-console-headless.sock"
    run_headless_boot_until "$SERIAL_LOG_CONSOLE" "$MON_SOCK_CONSOLE" "$TEST_TIMEOUT_SECS" \
      'RAYOS_VMM:VMX:UNSUPPORTED' \
      'RAYOS_VMM:VIRTIO_CONSOLE:RECV|G:CONSOLE'
    tr -d '\r' < "$SERIAL_LOG_CONSOLE" > "$SERIAL_NORM_CONSOLE" 2>/dev/null || true
    if grep -F -a -q "RAYOS_VMM:VIRTIO_CONSOLE:RECV" "$SERIAL_NORM_CONSOLE" || grep -F -a -q "G:CONSOLE" "$SERIAL_NORM_CONSOLE"; then
      echo "PASS: guest-driven virtio-console message observed" >&2
    else
      echo "NOTE: guest-driven console message not observed; check guest blob generation" >&2
    fi
  fi

  # Optional: exercise IRQ injection fallback path by forcing VMWRITE to fail
  # and verifying we either simulate or perform a LAPIC-based injection.
  SERIAL_LOG_FORCE="$WORK_DIR/serial-vmm-hypervisor-boot.force.log"
  SERIAL_NORM_FORCE="$WORK_DIR/serial-vmm-hypervisor-boot.force.norm.log"
  : > "$SERIAL_LOG_FORCE"
  RAYOS_KERNEL_FEATURES_FORCE="${RAYOS_KERNEL_FEATURES},vmm_inject_force_fail"
  echo "Running forced-inject test features: $RAYOS_KERNEL_FEATURES_FORCE" >&2
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  RUSTC="$(rustup which rustc)" cargo build \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --release \
    --target x86_64-unknown-none \
    --features "$RAYOS_KERNEL_FEATURES_FORCE" \
    >/dev/null
  popd >/dev/null

  # Run a headless boot with forced-failure features; keep old SERIAL_LOG untouched.
  MON_SOCK_FORCE="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-force-headless.sock"
  run_headless_boot_until "$SERIAL_LOG_FORCE" "$MON_SOCK_FORCE" "$TEST_TIMEOUT_SECS" \
    'RAYOS_VMM:VMX:UNSUPPORTED' \
    'RAYOS_VMM:VMX:FORCED_VMWRITE_FAIL' \
    'RAYOS_VMM:VMX:INJECT_VIA_LAPIC_SIM|RAYOS_VMM:VMX:INJECT_VIA_LAPIC|RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_PENDING|RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_OK|RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_FAILED_MAX'
  tr -d '\r' < "$SERIAL_LOG_FORCE" > "$SERIAL_NORM_FORCE" 2>/dev/null || true
  if grep -F -a -q "RAYOS_VMM:VMX:FORCED_VMWRITE_FAIL" "$SERIAL_NORM_FORCE" && \
     (grep -F -a -q "RAYOS_VMM:VMX:INJECT_VIA_LAPIC_SIM" "$SERIAL_NORM_FORCE" || grep -F -a -q "RAYOS_VMM:VMX:INJECT_VIA_LAPIC" "$SERIAL_NORM_FORCE" || grep -F -a -q "RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_PENDING" "$SERIAL_NORM_FORCE" || grep -F -a -q "RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_OK" "$SERIAL_NORM_FORCE" || grep -F -a -q "RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_FAILED_MAX" "$SERIAL_NORM_FORCE"); then
    echo "PASS: IRQ injection fallback exercised (LAPIC or bounded retry)" >&2
  else
    echo "NOTE: IRQ injection fallback not exercised; check LAPIC mapping or test flags" >&2
  fi

  # Forced MSI run: exercise the MSI fallback path (when supported by test flags).
  SERIAL_LOG_FORCE_MSI="$WORK_DIR/serial-vmm-hypervisor-boot.force-msi.log"
  SERIAL_NORM_FORCE_MSI="$WORK_DIR/serial-vmm-hypervisor-boot.force-msi.norm.log"
  : > "$SERIAL_LOG_FORCE_MSI"
  RAYOS_KERNEL_FEATURES_FORCE_MSI="${RAYOS_KERNEL_FEATURES},vmm_inject_force_fail,vmm_inject_force_msi_fail"
  echo "Running forced-MSI test features: $RAYOS_KERNEL_FEATURES_FORCE_MSI" >&2
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  RUSTC="$(rustup which rustc)" cargo build \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --release \
    --target x86_64-unknown-none \
    --features "$RAYOS_KERNEL_FEATURES_FORCE_MSI" \
    >/dev/null
  popd >/dev/null

  # Run a headless boot with forced-MSI features; keep old SERIAL_LOG untouched.
  MON_SOCK_FORCE_MSI="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-force-msi-headless.sock"
  run_headless_boot_until "$SERIAL_LOG_FORCE_MSI" "$MON_SOCK_FORCE_MSI" "$TEST_TIMEOUT_SECS" \
    'RAYOS_VMM:VMX:UNSUPPORTED' \
    'RAYOS_VMM:VMX:FORCED_MSI_INJECT' \
    'RAYOS_VMM:VMX:INJECT_VIA_MSI_SIM|RAYOS_VMM:VMX:INJECT_VIA_MSI'
  tr -d '\r' < "$SERIAL_LOG_FORCE_MSI" > "$SERIAL_NORM_FORCE_MSI" 2>/dev/null || true
  if grep -F -a -q "RAYOS_VMM:VMX:FORCED_MSI_INJECT" "$SERIAL_NORM_FORCE_MSI" && \
     (grep -F -a -q "RAYOS_VMM:VMX:INJECT_VIA_MSI_SIM" "$SERIAL_NORM_FORCE_MSI" || grep -F -a -q "RAYOS_VMM:VMX:INJECT_VIA_MSI" "$SERIAL_NORM_FORCE_MSI"); then
    echo "PASS: IRQ injection MSI fallback exercised" >&2
  else
    echo "NOTE: IRQ injection MSI fallback not exercised; check MSI mapping or test flags" >&2
  fi

  # Forced backoff selftest: build/runnable smoke run that verifies backoff counters and logs.
  SERIAL_LOG_FORCE_BO="$WORK_DIR/serial-vmm-hypervisor-boot.force-bo.log"
  SERIAL_NORM_FORCE_BO="$WORK_DIR/serial-vmm-hypervisor-boot.force-bo.norm.log"
  : > "$SERIAL_LOG_FORCE_BO"
  RAYOS_KERNEL_FEATURES_FORCE_BO="${RAYOS_KERNEL_FEATURES},vmm_inject_force_all_fail,vmm_inject_backoff_selftest"
  echo "Running forced-backoff test features: $RAYOS_KERNEL_FEATURES_FORCE_BO" >&2
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  RUSTC="$(rustup which rustc)" cargo build \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --release \
    --target x86_64-unknown-none \
    --features "$RAYOS_KERNEL_FEATURES_FORCE_BO" \
    >/dev/null
  popd >/dev/null

  # Run the boot where the selftest should execute and leave diagnostic logs.
  MON_SOCK_FORCE_BO="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-force-bo-headless.sock"
  run_headless_boot_until "$SERIAL_LOG_FORCE_BO" "$MON_SOCK_FORCE_BO" "$TEST_TIMEOUT_SECS" \
    'RAYOS_VMM:VMX:UNSUPPORTED' \
    'RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_BEGIN' \
    'RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_END|RAYOS_VIRTIO_MMIO:BACKOFF_SELFTEST_END'
  tr -d '\r' < "$SERIAL_LOG_FORCE_BO" > "$SERIAL_NORM_FORCE_BO" 2>/dev/null || true
  if grep -F -a -q "RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_BEGIN" "$SERIAL_NORM_FORCE_BO" && \
     grep -F -a -q "RAYOS_VIRTIO_MMIO:BACKOFF_SELFTEST_END" "$SERIAL_NORM_FORCE_BO" || \
     grep -F -a -q "RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_END" "$SERIAL_NORM_FORCE_BO"; then
    echo "PASS: IRQ injection backoff selftest exercised" >&2
  else
    echo "NOTE: IRQ injection backoff selftest not observed; check build flags" >&2
  fi

  # Optional: virtio-blk image-backed initialization (P1 follow-up)
  SERIAL_LOG_IMG="$WORK_DIR/serial-vmm-hypervisor-boot.blkimg.log"
  SERIAL_NORM_IMG="$WORK_DIR/serial-vmm-hypervisor-boot.blkimg.norm.log"
  : > "$SERIAL_LOG_IMG"
  # IMPORTANT: this check must boot a virtio-blk device. If we include vmm_virtio_console
  # (or vmm_virtio_gpu), the VMM will expose a different device_id and the blk disk init
  # path won't run.
  RAYOS_KERNEL_FEATURES_IMG="vmm_hypervisor,vmm_hypervisor_smoke,vmm_virtio_blk_image"
  echo "Running virtio-blk image init features: $RAYOS_KERNEL_FEATURES_IMG" >&2
  "$ROOT_DIR/scripts/generate-vmm-disk-image.sh" >/dev/null
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  RUSTC="$(rustup which rustc)" cargo build \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --release \
    --target x86_64-unknown-none \
    --features "$RAYOS_KERNEL_FEATURES_IMG" \
    >/dev/null
  popd >/dev/null

  MON_SOCK_IMG="$WORK_DIR/qemu-monitor-vmm-hypervisor-boot-blkimg-headless.sock"
  run_headless_boot_until "$SERIAL_LOG_IMG" "$MON_SOCK_IMG" "$TEST_TIMEOUT_SECS" \
    'RAYOS_VMM:VMX:UNSUPPORTED' \
    'RAYOS_VMM:VIRTIO_BLK:DISK_INIT_IMAGE'
  tr -d '\r' < "$SERIAL_LOG_IMG" > "$SERIAL_NORM_IMG" 2>/dev/null || true
  if grep -F -a -q "RAYOS_VMM:VIRTIO_BLK:DISK_INIT_IMAGE" "$SERIAL_NORM_IMG"; then
    echo "PASS: virtio-blk initialized from embedded disk image" >&2
  else
    echo "NOTE: virtio-blk image init marker not observed" >&2
  fi

  echo "Serial log: $SERIAL_LOG" >&2
  exit 0
fi

echo "FAIL: missing expected VMX markers" >&2
echo "Serial log: $SERIAL_LOG" >&2
tail -n 200 "$SERIAL_NORM" 2>/dev/null || true
exit 1
