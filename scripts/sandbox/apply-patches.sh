#!/usr/bin/env bash
set -euo pipefail

# Apply the relaxed capabilities-version client patch into the PhosSpace PM3
# source sandbox.
#
# Patches are applied in .build-env/pm3-src/ only; no host toolchain tree
# is read, written, or locked.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SANDBOX_ROOT="$PROJECT_ROOT/.build-env"
PM3_SRC="$SANDBOX_ROOT/pm3-src"
PATCH_FILE="$PROJECT_ROOT/src-tauri/src/client_patch/comms_relaxed_version.patch"

cd "$PM3_SRC"

if [ ! -f "$PATCH_FILE" ]; then
    echo "Patch file not found: $PATCH_FILE" >&2
    exit 1
fi

git apply "$PATCH_FILE"
echo "Applied patch: $PATCH_FILE"
