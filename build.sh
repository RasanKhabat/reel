#!/bin/zsh
# Builds Reel.app (Rust). Usage: ./build.sh [output-dir]   (default: build)
# Needs only Rust (cargo) and Apple's Command Line Tools; no Xcode.
set -euo pipefail
cd "$(dirname "$0")"
OUT="${1:-build}"
cargo build --release
APP="$OUT/Reel.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/Reel "$APP/Contents/MacOS/Reel"
cp macos/Info.plist "$APP/Contents/Info.plist"
# Sign with the local self-signed identity if present, so macOS keeps the
# Accessibility approval across rebuilds; otherwise fall back to ad-hoc.
IDENTITY="Reel Local Signing"
if security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
  codesign --force --sign "$IDENTITY" "$APP"
else
  echo "warning: '$IDENTITY' not found; ad-hoc signing (Accessibility must be re-approved after each build)"
  codesign --force --sign - "$APP"
fi
echo "Built $APP"
