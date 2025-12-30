#!/bin/bash
# Convenience launcher: Alpine ISO + networking + (optional) virgl.
#
# This is the fastest way to get an on-screen Linux Wayland session
# you can tinker with while the real RayOS<->Wayland bridge is built.

set -euo pipefail

# Enable networking so apk can fetch packages.
export LINUX_NET=1

# Optional: set LINUX_GL=1 for virgl (can improve weston performance).
# export LINUX_GL=1

exec "$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)/run-linux-subsystem-alpine-iso.sh"
