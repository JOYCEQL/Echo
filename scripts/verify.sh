#!/bin/zsh
set -euo pipefail
cd "${0:A:h:h}"
cargo test --locked
cargo build --release --locked
