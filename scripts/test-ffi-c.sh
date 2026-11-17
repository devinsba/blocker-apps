#!/usr/bin/env bash
# Builds blocker-ffi as a static library and links the C smoke test against it.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo build -p blocker-ffi --release
LIB_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/release"
OUT="$(mktemp -d)"
EXTRA=()
case "$(uname -s)" in
  Darwin) EXTRA=(-framework CoreFoundation -framework Security) ;;
  Linux) EXTRA=(-lpthread -ldl -lm) ;;
esac
cc -std=c11 -Wall -Werror -I crates/blocker-ffi/include crates/blocker-ffi/examples/smoke.c \
  "$LIB_DIR/libblocker_ffi.a" "${EXTRA[@]}" -o "$OUT/smoke"
"$OUT/smoke"
