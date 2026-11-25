#!/bin/zsh
set -euo pipefail
cd "${0:A:h:h}"
ICON_WORK=$(mktemp -d)
trap 'rm -rf "$ICON_WORK"' EXIT
mkdir "$ICON_WORK/Echo.iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" packaging/Echo.png --out "$ICON_WORK/Echo.iconset/icon_${size}x${size}.png" >/dev/null
  retina=$((size * 2))
  sips -z "$retina" "$retina" packaging/Echo.png --out "$ICON_WORK/Echo.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICON_WORK/Echo.iconset" -o packaging/Echo.icns
mkdir -p "$ICON_WORK/Echo.app/Contents/Resources" "$ICON_WORK/Echo.app/Contents/MacOS"
# A valid executable prevents NSWorkspace from adding an incompatible-app badge.
cp /usr/bin/true "$ICON_WORK/Echo.app/Contents/MacOS/Echo"
cp packaging/Info.plist "$ICON_WORK/Echo.app/Contents/Info.plist"
cp packaging/Echo.icns "$ICON_WORK/Echo.app/Contents/Resources/Echo.icns"
swift scripts/render-app-icon.swift "$ICON_WORK/Echo.app" packaging/Echo-runtime.png
