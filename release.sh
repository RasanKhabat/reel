#!/bin/zsh
# Builds a downloadable release: dist/Reel-<version>-macos-arm64.zip (+ .sha256), plus a
# copy named dist/Reel-macos-arm64.zip.
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
xattr -cr "$APP"
codesign --force --sign - "$APP"
mkdir -p dist
ZIP="dist/Reel-$VERSION-macos-arm64.zip"
rm -f "$ZIP"
ditto -c -k --norsrc --noextattr --keepParent "$APP" "$ZIP"
(cd dist && shasum -a 256 "$(basename "$ZIP")") | tee "$ZIP.sha256"
# Same zip under a fixed name, so .../releases/latest/download/Reel-macos-arm64.zip always
# fetches the newest version (used by the README's Terminal install).
cp "$ZIP" dist/Reel-macos-arm64.zip
rm -rf "$STAGE"
echo "Release: $ZIP"
