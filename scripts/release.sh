#!/bin/bash
# The publisher signs finished packages together on the release machine.
set -euo pipefail
cd "$(dirname "$0")/.."
exec bash scripts/build-macos.sh "$@"
