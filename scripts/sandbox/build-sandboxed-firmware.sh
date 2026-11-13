#!/usr/bin/env bash
set -euo pipefail

# Build the PM5 firmware (BWM variant) inside the PhosSpace sandbox using the
# seeded headless micro-toolchain at .build-env/toolchain/.
#
# Executed via scripts/windows/build-sandboxed-client.ps1 ($localBash).
# All make invocations are capped at -j4 to avoid MSYS2 memory spikes.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PM3_SRC="$PROJECT_ROOT/.build-env/pm3-src"

cd "$PM3_SRC"

# Firmware build (BWM variant)
make clean PLATFORM=PM5 PLATFORM_EXTRAS=BWM
make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM
