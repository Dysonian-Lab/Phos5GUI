#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOOLCHAIN="$PROJECT_ROOT/.build-env/toolchain"
export PATH="$TOOLCHAIN/usr/bin:$TOOLCHAIN/mingw64/bin:$TOOLCHAIN/opt/arm-none-eabi/bin:$PATH"
FW="$PROJECT_ROOT/src-tauri/firmware/pm5"
echo "=== Standard firmware sections ==="
arm-none-eabi-readelf -S "$FW/standard/fullimage.elf"
echo ""
echo "=== BWM firmware sections ==="
arm-none-eabi-readelf -S "$FW/bwm/fullimage.elf"
