#!/usr/bin/env bash
# Cross-compiles blocker-ffi (Rust core + C ABI) for Android and copies the
# static libraries to where mobile/android/blocker-core's CMakeLists.txt
# expects them:
#
#   mobile/android/blocker-core/src/main/jniLibs-static/<abi>/libblocker_ffi.a
#
# Usage:
#   ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/27.2.12479018 scripts/build-android.sh
#   ABIS="arm64-v8a x86_64" scripts/build-android.sh     # subset of ABIs
#   PROFILE=debug scripts/build-android.sh               # unoptimized build
#
# Then build the app: (cd mobile/android && ./gradlew assembleDebug)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_ROOT="$ROOT/mobile/android/blocker-core/src/main/jniLibs-static"
API_LEVEL="${ANDROID_API_LEVEL:-26}" # must match minSdk in mobile/android
PROFILE="${PROFILE:-release}"
ABIS="${ABIS:-arm64-v8a armeabi-v7a x86_64 x86}"
export PATH="$HOME/.cargo/bin:$PATH"

die() {
  echo "error: $*" >&2
  exit 1
}

# --- NDK ---------------------------------------------------------------------
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" ]]; then
  die "ANDROID_NDK_HOME is not set.
  Install the NDK (Android Studio > SDK Manager > SDK Tools > NDK (Side by side),
  or: sdkmanager 'ndk;27.2.12479018') and point ANDROID_NDK_HOME at it, e.g.
    export ANDROID_NDK_HOME=\$HOME/Library/Android/sdk/ndk/27.2.12479018"
fi
[[ -d "$NDK" ]] || die "ANDROID_NDK_HOME=$NDK does not exist"

case "$(uname -s)" in
  Darwin) HOST_TAG=darwin-x86_64 ;; # also used on Apple silicon
  Linux) HOST_TAG=linux-x86_64 ;;
  MINGW* | MSYS* | CYGWIN*) HOST_TAG=windows-x86_64 ;;
  *) die "unsupported host OS $(uname -s)" ;;
esac
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
[[ -d "$TOOLCHAIN" ]] || die "NDK toolchain not found at $TOOLCHAIN (is ANDROID_NDK_HOME an NDK r23+ directory?)"
AR="$TOOLCHAIN/llvm-ar"
[[ -x "$AR" ]] || die "llvm-ar not found in $TOOLCHAIN"

command -v cargo >/dev/null || die "cargo not found (install Rust with rustup; ~/.cargo/bin is added to PATH)"
command -v rustup >/dev/null || die "rustup not found (needed to add Android targets)"

# --- targets -----------------------------------------------------------------
# abi -> Rust target triple, and the NDK clang prefix (armv7 differs).
triple_for() {
  case "$1" in
    arm64-v8a) echo aarch64-linux-android ;;
    armeabi-v7a) echo armv7-linux-androideabi ;;
    x86_64) echo x86_64-linux-android ;;
    x86) echo i686-linux-android ;;
    *) die "unknown ABI '$1' (expected arm64-v8a, armeabi-v7a, x86_64, x86)" ;;
  esac
}
clang_for() {
  case "$1" in
    armv7-linux-androideabi) echo "$TOOLCHAIN/armv7a-linux-androideabi${API_LEVEL}-clang" ;;
    *) echo "$TOOLCHAIN/$1${API_LEVEL}-clang" ;;
  esac
}

TRIPLES=()
for abi in $ABIS; do TRIPLES+=("$(triple_for "$abi")"); done
rustup target add "${TRIPLES[@]}"

case "$PROFILE" in
  release) PROFILE_FLAG=(--release); PROFILE_DIR=release ;;
  debug | dev) PROFILE_FLAG=(); PROFILE_DIR=debug ;;
  *) PROFILE_FLAG=(--profile "$PROFILE"); PROFILE_DIR="$PROFILE" ;;
esac
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# --- build -------------------------------------------------------------------
cd "$ROOT"
for abi in $ABIS; do
  triple="$(triple_for "$abi")"
  cc="$(clang_for "$triple")"
  [[ -x "$cc" ]] || die "$cc not found (does this NDK support API $API_LEVEL?)"
  upper="$(echo "$triple" | tr 'a-z-' 'A-Z_')"
  lower="${triple//-/_}"

  echo "==> $abi ($triple, API $API_LEVEL)"
  # The linker is used for the cdylib crate type; CC/AR for any C code built
  # by dependencies' build scripts (cc crate).
  env \
    "CARGO_TARGET_${upper}_LINKER=$cc" \
    "CC_${lower}=$cc" \
    "AR_${lower}=$AR" \
    cargo build -p blocker-ffi --target "$triple" ${PROFILE_FLAG[@]+"${PROFILE_FLAG[@]}"}

  lib="$TARGET_DIR/$triple/$PROFILE_DIR/libblocker_ffi.a"
  [[ -f "$lib" ]] || die "expected $lib after build"
  mkdir -p "$OUT_ROOT/$abi"
  cp "$lib" "$OUT_ROOT/$abi/libblocker_ffi.a"
  echo "    -> ${OUT_ROOT#"$ROOT/"}/$abi/libblocker_ffi.a"
done

echo "Done. Build the app with: (cd mobile/android && ./gradlew :app:assembleDebug)"
