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

# Reuse the transparent application icon for the Windows shell and README.
sips -z 176 176 packaging/Echo-runtime.png --out docs/images/echo-icon.png >/dev/null
# Keep the rounded tile but trim the Dock padding for small taskbar icons.
sips -z 1024 1024 packaging/Echo-runtime.png --out "$ICON_WORK/dock.png" >/dev/null
sips -c 896 896 "$ICON_WORK/dock.png" --out "$ICON_WORK/taskbar.png" >/dev/null
for size in 16 24 32 48 64 128 256; do
  sips -z "$size" "$size" "$ICON_WORK/taskbar.png" --out "$ICON_WORK/win-$size.png" >/dev/null
done
python3 - "$ICON_WORK" <<'ICO'
from pathlib import Path
import struct
import sys

images = sorted(Path(sys.argv[1]).glob("win-*.png"), key=lambda p: int(p.stem[4:]))
header = struct.pack("<HHH", 0, 1, len(images))
directory = bytearray()
pixels = bytearray()
for path in images:
    side = int(path.stem[4:]) % 256  # ICO represents 256 pixels with a zero byte.
    data = path.read_bytes()
    offset = len(header) + 16 * len(images) + len(pixels)
    directory.extend(struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset))
    pixels.extend(data)
Path("packaging/Echo.ico").write_bytes(header + directory + pixels)
ICO
