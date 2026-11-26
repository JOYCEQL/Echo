#!/bin/bash
# The publisher signs finished packages together on the release machine.
set -euo pipefail
cd "$(dirname "$0")/.."
export ECHO_UPDATE_PUBLIC_KEY
ECHO_UPDATE_PUBLIC_KEY=$(sed -n '2p' packaging/echo-update.pub)
exec bash scripts/build-macos.sh "$@"
