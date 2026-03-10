#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/ide-wasm"
TARGET_DIR="$CRATE_DIR/target"
DIST_DIR="$CRATE_DIR/dist"

cargo build \
  --manifest-path "$CRATE_DIR/Cargo.toml" \
  --lib \
  --release \
  --target wasm32-unknown-unknown \
  --target-dir "$TARGET_DIR"

mkdir -p "$DIST_DIR"

wasm-bindgen \
  "$TARGET_DIR/wasm32-unknown-unknown/release/palmscript_ide_wasm.wasm" \
  --out-dir "$DIST_DIR" \
  --out-name palmscript_ide \
  --target web
