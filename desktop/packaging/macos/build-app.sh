#!/usr/bin/env bash
# Builds, assembles, signs (and optionally notarizes) Blocker.app.
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  build-app.sh --team-id XXXXXXXXXX --identity "Developer ID Application: Name (TEAMID)" \\
               --app-profile app.provisionprofile --filter-profile filter.provisionprofile \\
               [--notarize-profile <notarytool keychain profile>] [--out dist]
  build-app.sh --adhoc [--team-id XXXXXXXXXX] [--out dist]

--bin-dir DIR skips cargo and takes blocker, blocker-filter and blocker-ui from DIR.
--adhoc signs with "-" and embeds no provisioning profiles (structure testing
only; the system extension will not activate unless SIP/developer mode is off).
USAGE
}

TEAM_ID=""
IDENTITY=""
APP_PROFILE=""
FILTER_PROFILE=""
NOTARIZE_PROFILE=""
OUT="dist"
ADHOC=0
BIN_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --team-id) TEAM_ID="${2:?}"; shift 2 ;;
    --identity) IDENTITY="${2:?}"; shift 2 ;;
    --app-profile) APP_PROFILE="${2:?}"; shift 2 ;;
    --filter-profile) FILTER_PROFILE="${2:?}"; shift 2 ;;
    --notarize-profile) NOTARIZE_PROFILE="${2:?}"; shift 2 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    --adhoc) ADHOC=1; shift ;;
    --bin-dir) BIN_DIR="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ $ADHOC -eq 1 ]]; then
  IDENTITY="-"
  TEAM_ID="${TEAM_ID:-ADHOCTEAM0}"
else
  if [[ -z $TEAM_ID || -z $IDENTITY || -z $APP_PROFILE || -z $FILTER_PROFILE ]]; then
    echo "error: --team-id, --identity, --app-profile and --filter-profile are required (or use --adhoc)" >&2
    usage >&2
    exit 2
  fi
  [[ -f $APP_PROFILE ]] || { echo "error: no such file: $APP_PROFILE" >&2; exit 2; }
  [[ -f $FILTER_PROFILE ]] || { echo "error: no such file: $FILTER_PROFILE" >&2; exit 2; }
  # Resolve to absolute paths before any cd.
  APP_PROFILE="$(cd "$(dirname "$APP_PROFILE")" && pwd)/$(basename "$APP_PROFILE")"
  FILTER_PROFILE="$(cd "$(dirname "$FILTER_PROFILE")" && pwd)/$(basename "$FILTER_PROFILE")"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)"
VERSION="${VERSION:-0.1.0}"

if [[ -n $BIN_DIR ]]; then
  TARGET_DIR="$BIN_DIR"
else
  echo "==> Building release binaries"
  (cd "$ROOT" && cargo build --release -p blocker -p blocker-filter -p blocker-ui)
  TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/release"
fi

APP="$OUT/Blocker.app"
EXT="$APP/Contents/Library/SystemExtensions/com.briandevins.blocker.filter.systemextension"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$EXT/Contents/MacOS"

echo "==> Assembling bundle"
cp "$TARGET_DIR/blocker-ui" "$APP/Contents/MacOS/blocker-ui"   # main executable (menu-bar UI)
cp "$TARGET_DIR/blocker" "$APP/Contents/MacOS/blocker"         # CLI
cp "$TARGET_DIR/blocker-filter" "$EXT/Contents/MacOS/com.briandevins.blocker.filter"

subst() { # subst <template> <dest>
  sed -e "s/__TEAM_ID__/$TEAM_ID/g" -e "s/__VERSION__/$VERSION/g" "$1" > "$2"
}
subst "$SCRIPT_DIR/app/Info.plist" "$APP/Contents/Info.plist"
subst "$SCRIPT_DIR/filter/Info.plist" "$EXT/Contents/Info.plist"

WORK="$OUT/.work"
rm -rf "$WORK"
mkdir -p "$WORK"
subst "$SCRIPT_DIR/app.entitlements" "$WORK/app.entitlements"
subst "$SCRIPT_DIR/filter.entitlements" "$WORK/filter.entitlements"

plutil -lint "$APP/Contents/Info.plist" "$EXT/Contents/Info.plist" \
  "$WORK/app.entitlements" "$WORK/filter.entitlements"

if [[ $ADHOC -eq 0 ]]; then
  cp "$APP_PROFILE" "$APP/Contents/embedded.provisionprofile"
  cp "$FILTER_PROFILE" "$EXT/Contents/embedded.provisionprofile"
fi

# Sign inside-out: extension, then the CLI helper, then the app (whose main
# executable, blocker-ui, is signed as part of the bundle).
echo "==> Signing"
if [[ $ADHOC -eq 1 ]]; then
  TS_FLAG=(--timestamp=none)
else
  TS_FLAG=(--timestamp)
fi
codesign --force --options runtime "${TS_FLAG[@]}" \
  --entitlements "$WORK/filter.entitlements" -s "$IDENTITY" "$EXT"
# macOS kills any executable whose signature claims restricted entitlements
# (Network Extension, system extension install, app groups) unless an
# embedded provisioning profile grants them. Ad-hoc builds have no profile,
# so their UI and CLI are signed without entitlements: they run locally, and
# filter activation fails with a clear "code signature invalid" error.
APP_ENT=(--entitlements "$WORK/app.entitlements")
if [[ $ADHOC -eq 1 ]]; then
  APP_ENT=()
fi
codesign --force --options runtime "${TS_FLAG[@]}" --identifier dev.blocker.cli \
  ${APP_ENT[@]+"${APP_ENT[@]}"} -s "$IDENTITY" "$APP/Contents/MacOS/blocker"
codesign --force --options runtime "${TS_FLAG[@]}" \
  ${APP_ENT[@]+"${APP_ENT[@]}"} -s "$IDENTITY" "$APP"

echo "==> Verifying"
codesign --verify --deep --strict --verbose=2 "$APP"

if [[ -n $NOTARIZE_PROFILE && $ADHOC -eq 0 ]]; then
  echo "==> Notarizing"
  ZIP="$OUT/Blocker.zip"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
  xcrun notarytool submit "$ZIP" --wait --keychain-profile "$NOTARIZE_PROFILE"
  xcrun stapler staple "$APP"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
  echo "Notarized archive: $ZIP"
fi

rm -rf "$WORK"

cat <<NEXT

Built: $APP

Next steps:
  sudo rm -rf /Applications/Blocker.app
  sudo ditto "$APP" /Applications/Blocker.app
  /Applications/Blocker.app/Contents/MacOS/blocker activate
NEXT
