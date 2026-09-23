#!/usr/bin/env bash
# Usage: scripts/build-macos.sh [--debug] [--run]
# Builds Selo.app. Accessibility/TCC only behaves for a bundle signed with a stable identity,
# so use the cert from scripts/make-signing-cert.sh; ad-hoc drops the grant on every rebuild.
set -euo pipefail

BUNDLE_ID="com.selo.app"
BUNDLE_NAME="Selo"
DEFAULT_SIGN_ID="Selo Dev"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/build/$BUNDLE_NAME.app"

profile="release"
run=0
for arg in "$@"; do
  case "$arg" in
    --debug) profile="debug" ;;
    --release) profile="release" ;;
    --run) run=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

cd "$ROOT"
if [ "$profile" = "release" ]; then
  cargo build --release -p selo
else
  cargo build -p selo
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/$profile/selo" "$APP/Contents/MacOS/$BUNDLE_NAME"

# Finder/Dock read Resources/AppIcon.icns, not a raw PNG.
iconset="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/assets/AppIcon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" "$ROOT/assets/AppIcon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleName</key><string>$BUNDLE_NAME</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleExecutable</key><string>$BUNDLE_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST

identity="${SELO_SIGN_ID:-}"
if [ -z "$identity" ]; then
  if security find-certificate -c "$DEFAULT_SIGN_ID" >/dev/null 2>&1; then
    identity="$DEFAULT_SIGN_ID"
  else
    identity="-"
    echo "warning: signing ad-hoc; the Accessibility grant is dropped on every rebuild." >&2
    echo "         Run scripts/make-signing-cert.sh once." >&2
  fi
fi
codesign --force --deep --sign "$identity" --identifier "$BUNDLE_ID" "$APP"
echo "bundle: $APP"
codesign -d -r- "$APP" 2>&1 | grep designated || true

[ "$run" -eq 1 ] || exit 0

# LaunchServices, not the binary directly: running Contents/MacOS/... attributes the process
# to the terminal, breaking TCC. --stdout /dev/stdout fails on a pipe, so log to a file.
log="$ROOT/build/$BUNDLE_NAME.log"
pkill -x "$BUNDLE_NAME" 2>/dev/null || true
pkill -f "tail -f $log" 2>/dev/null || true
: > "$log"
open "$APP" --stdout "$log" --stderr "$log"
echo "log: $log"

trap 'pkill -x "$BUNDLE_NAME"; exit 0' INT
tail -f "$log" &
tail_pid=$!
while pgrep -x "$BUNDLE_NAME" >/dev/null; do sleep 0.5; done
kill "$tail_pid" 2>/dev/null
