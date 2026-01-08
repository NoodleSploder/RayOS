#!/bin/bash
# Run all automated headless tests and report results

set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# List of all headless test scripts that should pass/fail deterministically
HEADLESS_TESTS=(
    "scripts/test-boot-headless.sh"
    "scripts/test-boot-aarch64-headless.sh"
    "scripts/test-boot-aarch64-ai-headless.sh"
    "scripts/test-boot-aarch64-kernel-headless.sh"
    "scripts/test-boot-aarch64-kernel-ai-headless.sh"
    "scripts/test-boot-aarch64-kernel-volume-headless.sh"
    "scripts/test-boot-aarch64-volume-headless.sh"
    "scripts/test-boot-ai-headless.sh"
    "scripts/test-boot-cortex-daemon-headless.sh"
    "scripts/test-boot-cortex-headless.sh"
    "scripts/test-boot-local-ai-headless.sh"
    "scripts/test-boot-local-ai-matrix-headless.sh"
    "scripts/test-boot-local-llm-headless.sh"
    "scripts/test-boot-rag-headless.sh"
    "scripts/test-bootloader-qemu.sh"
    "scripts/test-chainloading.sh"
    "scripts/test-conductor-autostart.sh"
    "scripts/test-conductor-snapshot.sh"
    "scripts/test-desktop-control-e2e-headless.sh"
    "scripts/test-dev-scanout.sh"
    "scripts/test-installer-boot-headless.sh"
)

TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
PASSED=0
FAILED=0
SKIPPED=0
FAILED_TESTS=()

echo "======================================"
echo "RayOS Test Suite - Headless Tests"
echo "======================================"
echo "Running ${#HEADLESS_TESTS[@]} tests with ${TIMEOUT_SECS}s timeout per test"
echo

for test_script in "${HEADLESS_TESTS[@]}"; do
    if [ ! -f "$test_script" ]; then
        echo "⊘ SKIP: $test_script (not found)"
        ((SKIPPED++))
        continue
    fi

    test_name="$(basename "$test_script")"

    # Run with timeout
    if timeout "$TIMEOUT_SECS" bash "$test_script" >/dev/null 2>&1; then
        echo "✓ PASS: $test_name"
        ((PASSED++))
    else
        exit_code=$?
        if [ $exit_code -eq 124 ]; then
            echo "✗ TIMEOUT: $test_name (>${TIMEOUT_SECS}s)"
        else
            echo "✗ FAIL: $test_name (exit code: $exit_code)"
        fi
        FAILED_TESTS+=("$test_name")
        ((FAILED++))
    fi
done

echo
echo "======================================"
echo "Test Summary"
echo "======================================"
echo "PASSED:  $PASSED"
echo "FAILED:  $FAILED"
echo "SKIPPED: $SKIPPED"
echo "TOTAL:   $((PASSED + FAILED + SKIPPED))"
echo

if [ $FAILED -gt 0 ]; then
    echo "Failed tests:"
    for test in "${FAILED_TESTS[@]}"; do
        echo "  - $test"
    done
    echo
    exit 1
else
    echo "All tests passed! ✓"
    exit 0
fi
