#!/bin/zsh
set -euo pipefail
cd "${0:A:h:h}"
zsh scripts/build-icon.sh
cargo build --release --locked
APP="../Echo.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/echo "$APP/Contents/MacOS/Echo"
cp packaging/Info.plist "$APP/Contents/Info.plist"
echo_version=$(python3 -c 'import json, subprocess; print(next(p["version"] for p in json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"]))["packages"] if p["name"] == "echo"))')
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $echo_version" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $echo_version" "$APP/Contents/Info.plist"
cp packaging/Echo.icns "$APP/Contents/Resources/Echo.icns"
cp -R licenses "$APP/Contents/Resources/"
cp LICENSE THIRD_PARTY.md "$APP/Contents/Resources/"
codesign --force --deep --sign - "$APP"
echo "Built: ${APP:A}"
