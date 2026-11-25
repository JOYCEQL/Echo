#!/bin/bash
# Produces a Minisign-authenticated, ad-hoc code-signed update. No paid Apple certificate.
# Does not upload or publish anything.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${ECHO_UPDATE_KEY_FILE:?Set the Minisign public key file}"
: "${MINISIGN_SECRET_KEY:?Set the Minisign private key file outside the repository}"
: "${ECHO_UPDATE_REPOSITORY:?Set the public GitHub release repository URL}"
export ECHO_UPDATE_REPOSITORY
python3 - "$ECHO_UPDATE_REPOSITORY" <<'PY'
import re, sys, json, urllib.request
url = sys.argv[1]
assert re.fullmatch(r'https://github.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', url), 'Invalid release repository'
assert all(p not in ('.', '..') for p in url.split('/')[-2:]), 'Invalid release repository'
with urllib.request.urlopen('https://api.github.com/repos/'+url.removeprefix('https://github.com/'), timeout=20) as response:
    assert json.load(response)['private'] is False, 'Release repository must be publicly accessible'
PY
command -v minisign >/dev/null
[[ -f "$ECHO_UPDATE_KEY_FILE" && -f "$MINISIGN_SECRET_KEY" ]]
export ECHO_UPDATE_PUBLIC_KEY
ECHO_UPDATE_PUBLIC_KEY=$(python3 - "$ECHO_UPDATE_KEY_FILE" <<'PY'
import base64, pathlib, sys
lines = pathlib.Path(sys.argv[1]).read_text().splitlines()
assert len(lines) == 2 and lines[0].startswith('untrusted comment:')
assert len(base64.b64decode(lines[1], validate=True)) == 42
print(lines[1])
PY
)
echo_version=$(python3 - <<'PY'
import re, json, subprocess
v = next(p['version'] for p in json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1']))['packages'] if p['name'] == 'echo')
assert re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', v)
print(v)
PY
)
echo_target=$(rustc -vV | sed -n 's/^host: //p')
case "$echo_target" in
  aarch64-apple-darwin) echo_arch=arm64 ;;
  x86_64-apple-darwin) echo_arch=x86_64 ;;
  *) echo 'Build releases on the target macOS architecture' >&2; exit 1 ;;
esac
bash scripts/build-macos.sh
echo_output="$PWD/dist"
echo_name="Echo-$echo_version-macos-$echo_arch.zip"
echo_package="$echo_output/$echo_name"
minisign -S -s "$MINISIGN_SECRET_KEY" -m "$echo_package" -t "$echo_name"
minisign -V -p "$ECHO_UPDATE_KEY_FILE" -m "$echo_package"
python3 - "$echo_package" "$echo_version" "macos-$echo_arch" <<'PY'
import json, pathlib, sys
package, version, platform = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
manifest = dict(version=version, platform=platform, size=package.stat().st_size,
                signature=pathlib.Path(str(package)+'.minisig').read_text())
assert 0 < manifest['size'] <= 512*1024*1024
(package.parent / f'update-{platform}.json').write_text(json.dumps(manifest, indent=2)+'\n')
PY
(cd "$echo_output" && shasum -a 256 "$echo_name" > "$echo_name.sha256")
printf 'Verified release artifacts: %s\n' "$echo_output"
