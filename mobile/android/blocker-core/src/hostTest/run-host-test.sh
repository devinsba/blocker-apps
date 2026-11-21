#!/usr/bin/env bash
# Builds blocker_jni.c as a host shared library linked against the host
# libblocker_ffi.a and runs HostTest.java against it with plain javac/java.
# Verifies the JNI shim (marshalling, freeing, exceptions) without Android.
#
#   mobile/android/blocker-core/src/hostTest/run-host-test.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

if [[ -z "${JAVA_HOME:-}" ]]; then
  JAVAC="$(command -v javac)" || { echo "error: javac not found; set JAVA_HOME" >&2; exit 1; }
  # Resolve symlinks (sdkman 'current', /usr/bin shims) to the JDK home.
  JAVA_HOME="$(cd "$(dirname "$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$JAVAC")")/.." && pwd)"
fi

(cd "$ROOT" && cargo build -p blocker-ffi --release)
LIB="${CARGO_TARGET_DIR:-$ROOT/target}/release/libblocker_ffi.a"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

case "$(uname -s)" in
  Darwin) PLATFORM=darwin; SO="$OUT/libblocker_jni.dylib"; LINK=(-dynamiclib -framework CoreFoundation -framework Security) ;;
  Linux)  PLATFORM=linux;  SO="$OUT/libblocker_jni.so";    LINK=(-shared -fPIC -lpthread -ldl -lm) ;;
  *) echo "unsupported host $(uname -s)" >&2; exit 1 ;;
esac

cc -std=c11 -Wall -Wextra -Werror -fvisibility=hidden \
  -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/$PLATFORM" \
  -I"$ROOT/crates/blocker-ffi/include" \
  "$HERE/../main/cpp/blocker_jni.c" "$LIB" "${LINK[@]}" -o "$SO"

javac -d "$OUT/classes" "$HERE"/java/dev/blocker/core/*.java
java -Xcheck:jni -Dblocker.jni.path="$SO" -cp "$OUT/classes" dev.blocker.core.HostTest
