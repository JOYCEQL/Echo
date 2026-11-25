#!/bin/bash
# Build a macOS app and ZIP; update-package signing is done by release.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=13.0
echo_version=$(python3 - <<'PY'
import json, re, subprocess
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1']))
version = next(p['version'] for p in metadata['packages'] if p['name'] == 'echo')
assert re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version)
print(version)
PY
)
echo_target=$(rustc -vV | sed -n 's/^host: //p')
case "$echo_target" in
  aarch64-apple-darwin) echo_arch=arm64 ;;
  x86_64-apple-darwin) echo_arch=x86_64 ;;
  *) echo 'Build macOS packages on the target macOS architecture' >&2; exit 1 ;;
esac
cargo build --release --locked
mkdir -p dist
echo_work=$(mktemp -d "$PWD/dist/build-$echo_version-$echo_arch.XXXXXX")
trap 'rm -rf "$echo_work"' EXIT
echo_app="$echo_work/Echo.app"
mkdir -p "$echo_app/Contents/MacOS" "$echo_app/Contents/Resources"
cp target/release/echo "$echo_app/Contents/MacOS/Echo"
cp packaging/Info.plist "$echo_app/Contents/Info.plist"
cp packaging/Echo.icns "$echo_app/Contents/Resources/Echo.icns"
cp LICENSE THIRD_PARTY.md "$echo_app/Contents/Resources/"
cp -R licenses "$echo_app/Contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $echo_version" "$echo_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $echo_version" "$echo_app/Contents/Info.plist"
lipo "$echo_app/Contents/MacOS/Echo" -verify_arch "$echo_arch"
codesign --force --sign - "$echo_app"
codesign --verify --deep --strict "$echo_app"
echo_name="Echo-$echo_version-macos-$echo_arch.zip"
ditto -c -k --keepParent "$echo_app" "$PWD/dist/$echo_name"
(cd dist && shasum -a 256 "$echo_name" > "$echo_name.sha256")
printf 'Verified macOS package: %s\n' "$PWD/dist/$echo_name"
