#!/bin/bash
set -euo pipefail

# Automated PM5 Firmware Build Pipeline (sandbox-only)
# Builds both Standard and BWM variants headlessly using the seeded toolchain.
# All paths resolve inside .build-env/; no host toolchain trees are referenced.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SANDBOX_ROOT="$PROJECT_ROOT/.build-env"
PM3_SRC="$SANDBOX_ROOT/pm3-src"
TOOLCHAIN="$SANDBOX_ROOT/toolchain"

export PATH="$TOOLCHAIN/usr/bin:$TOOLCHAIN/mingw64/bin:$TOOLCHAIN/opt/arm-none-eabi/bin:$PATH"

echo "=== Automated PM5 Firmware Build Pipeline (Firmware Only) ==="
echo "Project root: $PROJECT_ROOT"
echo "PM3 source:   $PM3_SRC"
echo ""

cd "$PM3_SRC"

# Standard
echo "=== Building Standard firmware (PLATFORM=PM5) ==="
make clean PLATFORM=PM5
make -j4 PLATFORM=PM5
mkdir -p "$PROJECT_ROOT/src-tauri/firmware/pm5/standard"
cp -f armsrc/obj/fullimage.stage1.elf "$PROJECT_ROOT/src-tauri/firmware/pm5/standard/bootrom.elf"
cp -f armsrc/obj/fullimage.elf       "$PROJECT_ROOT/src-tauri/firmware/pm5/standard/fullimage.elf"

# BWM
echo "=== Building BWM firmware (PLATFORM=PM5 PLATFORM_EXTRAS=BWM) ==="
make clean PLATFORM=PM5 PLATFORM_EXTRAS=BWM
make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM
mkdir -p "$PROJECT_ROOT/src-tauri/firmware/pm5/bwm"
cp -f armsrc/obj/fullimage.stage1.elf "$PROJECT_ROOT/src-tauri/firmware/pm5/bwm/bootrom.elf"
cp -f armsrc/obj/fullimage.elf       "$PROJECT_ROOT/src-tauri/firmware/pm5/bwm/fullimage.elf"

echo "=== Build complete ==="
