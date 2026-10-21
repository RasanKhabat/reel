#!/bin/zsh
# Builds a downloadable release: dist/Reel-<version>-macos-arm64.zip (+ .sha256).
# Release builds are ad-hoc signed: nobody else has your local signing certificate, and
# without a paid Apple Developer ID every unsigned download needs "Open Anyway" once anyway.
set -euo pipefail
cd "$(dirname "$0")"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release
STAGE=$(mktemp -d)
APP="$STAGE/Reel.app"
mkdir -p "$APP/Contents/MacOS"
cp target/release/Reel "$APP/Contents/MacOS/Reel"
cp macos/Info.plist "$APP/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
mkdir -p dist
ZIP="dist/Reel-$VERSION-macos-arm64.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
shasum -a 256 "$ZIP" | tee "$ZIP.sha256"
rm -rf "$STAGE"
echo "Release: $ZIP"
