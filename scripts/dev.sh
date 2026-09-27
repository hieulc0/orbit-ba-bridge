#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

echo "==> Checking cargo workspace..."
cargo check --workspace

echo "==> Running workspace tests..."
cargo test --workspace

if [[ "${1:-}" == "--run" ]]; then
    echo "==> Starting bridge-server on localhost..."
    cargo run --bin bridge-server
fi
