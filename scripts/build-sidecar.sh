#!/usr/bin/env bash
# Build the engine for one target triple and install it as the Tauri sidecar.
# Usage: scripts/build-sidecar.sh [target-triple]   (default: host triple)
# On macOS also copies the vendored smartctl into app/src-tauri/resources/ for local dev
# (CI places its own smartctl there; an existing file is left alone).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
EXT=""; case "$TRIPLE" in *windows*) EXT=".exe";; esac

( cd "$ROOT/engine" && cargo build --release --target "$TRIPLE" )

BIN_DIR="$ROOT/app/src-tauri/binaries"
mkdir -p "$BIN_DIR"
cp "$ROOT/engine/target/$TRIPLE/release/checkmydrive-engine$EXT" "$BIN_DIR/checkmydrive-engine-$TRIPLE$EXT"
chmod +x "$BIN_DIR/checkmydrive-engine-$TRIPLE$EXT"
echo "sidecar: $BIN_DIR/checkmydrive-engine-$TRIPLE$EXT"

case "$TRIPLE" in
  *apple-darwin)
    RES="$ROOT/app/src-tauri/resources"; mkdir -p "$RES"
    if [ ! -f "$RES/smartctl" ] && [ -f "$ROOT/engine/vendor/macos/smartctl" ]; then
      cp "$ROOT/engine/vendor/macos/smartctl" "$RES/smartctl"; chmod +x "$RES/smartctl"
      echo "smartctl: $RES/smartctl (from engine/vendor)"
    fi;;
esac
