#!/bin/bash
# Build the update ZIP and first-install DMG from the same signed app.
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
echo_target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
case "$echo_target" in
  aarch64-apple-darwin) echo_arch=arm64 ;;
  x86_64-apple-darwin) echo_arch=x86_64 ;;
  *) echo 'Expected an Apple Silicon or Intel macOS target' >&2; exit 1 ;;
esac
cargo build --target "$echo_target" --release --locked
mkdir -p dist
echo_work=$(mktemp -d "$PWD/dist/build-$echo_version-$echo_arch.XXXXXX")
trap 'rm -rf "$echo_work"' EXIT
echo_app="$echo_work/Echo.app"
mkdir -p "$echo_app/Contents/MacOS" "$echo_app/Contents/Resources"
cp "${CARGO_TARGET_DIR:-target}/$echo_target/release/echo" "$echo_app/Contents/MacOS/Echo"
cp packaging/Info.plist "$echo_app/Contents/Info.plist"
cp packaging/Echo.icns "$echo_app/Contents/Resources/Echo.icns"
cp LICENSE THIRD_PARTY.md "$echo_app/Contents/Resources/"
cp -R licenses "$echo_app/Contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $echo_version" "$echo_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $echo_version" "$echo_app/Contents/Info.plist"
lipo "$echo_app/Contents/MacOS/Echo" -verify_arch "$echo_arch"
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" || -n "${APPLE_NOTARY_PROFILE:-}" ]]; then
    : "${APPLE_SIGNING_IDENTITY:?Set the Developer ID Application identity}"
    : "${APPLE_NOTARY_PROFILE:?Set the notarytool keychain profile}"
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$echo_app"
    ditto -c -k --keepParent "$echo_app" "$echo_work/notary.zip"
    xcrun notarytool submit "$echo_work/notary.zip" --keychain-profile "$APPLE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$echo_app"
    xcrun stapler validate "$echo_app"
else
    codesign --force --sign - "$echo_app"
fi
codesign --verify --deep --strict "$echo_app"
echo_name="Echo-$echo_version-macos-$echo_arch.zip"
ditto -c -k --keepParent "$echo_app" "$PWD/dist/$echo_name"
mkdir "$echo_work/image"
mv "$echo_app" "$echo_work/image/Echo.app"
ln -s /Applications "$echo_work/image/Applications"
echo_dmg="Echo-$echo_version-macos-$echo_arch.dmg"
hdiutil create -ov -fs HFS+ -format UDZO -volname Echo -srcfolder "$echo_work/image" "$PWD/dist/$echo_dmg"
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    codesign --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$PWD/dist/$echo_dmg"
    xcrun notarytool submit "$PWD/dist/$echo_dmg" --keychain-profile "$APPLE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$PWD/dist/$echo_dmg"
    xcrun stapler validate "$PWD/dist/$echo_dmg"
fi
hdiutil verify "$PWD/dist/$echo_dmg"
(cd dist && shasum -a 256 "$echo_dmg" > "$echo_dmg.sha256")
(cd dist && shasum -a 256 "$echo_name" > "$echo_name.sha256")
printf 'Verified macOS package: %s\n' "$PWD/dist/$echo_name"
