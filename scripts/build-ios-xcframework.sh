#!/usr/bin/env bash
# Builds blocker-ffi for Apple platforms.
#
#   scripts/build-ios-xcframework.sh            # iOS device + iOS simulator + macOS slices
#                                               # -> mobile/ios/Frameworks/BlockerFFI.xcframework
#   scripts/build-ios-xcframework.sh --macos    # host-only static lib for `swift test`
#                                               # (no Xcode needed; see mobile/ios/README.md)
#
# The full build needs Xcode (not just the Command Line Tools) because the iOS
# SDKs and `xcodebuild -create-xcframework` ship only with Xcode.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export PATH="$HOME/.cargo/bin:$PATH"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
INCLUDE_DIR="$ROOT/crates/blocker-ffi/include"
OUT_DIR="$ROOT/mobile/ios/Frameworks"
XCFRAMEWORK="$OUT_DIR/BlockerFFI.xcframework"
LIB=libblocker_ffi.a

die() { echo "error: $*" >&2; exit 1; }

usage() {
  sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

MODE=ios
case "${1:-}" in
  "") ;;
  --macos) MODE=macos ;;
  -h|--help) usage; exit 0 ;;
  *) usage >&2; die "unknown option: $1" ;;
esac

command -v cargo >/dev/null || die "cargo not found (install Rust with rustup; cargo lives in ~/.cargo/bin)"

if [[ "$MODE" == macos ]]; then
  # Host build only: what BLOCKER_FFI_LOCAL=1 in BlockerKit/Package.swift links.
  cargo build -p blocker-ffi --release
  [[ -f "$TARGET_DIR/release/$LIB" ]] || die "expected $TARGET_DIR/release/$LIB"
  echo
  echo "Built $TARGET_DIR/release/$LIB"
  echo "Run the wrapper tests with:"
  echo "  cd mobile/ios/BlockerKit && BLOCKER_FFI_LOCAL=1 swift test"
  exit 0
fi

# --- Full build: requires Xcode -------------------------------------------------
DEV_DIR="$(xcode-select -p 2>/dev/null || true)"
if [[ -z "$DEV_DIR" || "$DEV_DIR" == *CommandLineTools* ]] || ! xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1; then
  die "full Xcode is not selected (xcode-select -p: ${DEV_DIR:-none}).
  Install Xcode, then run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
  (For host-only testing without Xcode use: $0 --macos)"
fi
command -v xcodebuild >/dev/null || die "xcodebuild not found"
command -v rustup >/dev/null || die "rustup not found (needed to add the iOS targets)"

IOS_DEVICE=aarch64-apple-ios
IOS_SIM=(aarch64-apple-ios-sim x86_64-apple-ios)
MACOS=(aarch64-apple-darwin x86_64-apple-darwin)
ALL_TARGETS=("$IOS_DEVICE" "${IOS_SIM[@]}" "${MACOS[@]}")

INSTALLED="$(rustup target list --installed)"
MISSING=()
for t in "${ALL_TARGETS[@]}"; do
  grep -qx "$t" <<<"$INSTALLED" || MISSING+=("$t")
done
if ((${#MISSING[@]})); then
  rustup target add "${MISSING[@]}"
fi

export IPHONEOS_DEPLOYMENT_TARGET=16.0
export MACOSX_DEPLOYMENT_TARGET=13.0
for t in "${ALL_TARGETS[@]}"; do
  echo "==> cargo build -p blocker-ffi --release --target $t"
  cargo build -p blocker-ffi --release --target "$t"
done

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# Fat libraries for the simulator and macOS (one slice per platform+variant).
mkdir -p "$STAGE/ios-sim" "$STAGE/macos"
slices() { for t in "$@"; do printf '%s\n' "$TARGET_DIR/$t/release/$LIB"; done; }
lipo -create $(slices "${IOS_SIM[@]}") -output "$STAGE/ios-sim/$LIB"
lipo -create $(slices "${MACOS[@]}") -output "$STAGE/macos/$LIB"

# Headers go in a BlockerFFI/ subdirectory so the module map doesn't collide
# with other XCFrameworks that also ship a top-level module.modulemap.
HEADERS="$STAGE/Headers"
mkdir -p "$HEADERS/BlockerFFI"
cp "$INCLUDE_DIR/blocker.h" "$INCLUDE_DIR/module.modulemap" "$HEADERS/BlockerFFI/"

rm -rf "$XCFRAMEWORK"
mkdir -p "$OUT_DIR"
xcodebuild -create-xcframework \
  -library "$TARGET_DIR/$IOS_DEVICE/release/$LIB" -headers "$HEADERS" \
  -library "$STAGE/ios-sim/$LIB" -headers "$HEADERS" \
  -library "$STAGE/macos/$LIB" -headers "$HEADERS" \
  -output "$XCFRAMEWORK"

echo
echo "Created $XCFRAMEWORK"
