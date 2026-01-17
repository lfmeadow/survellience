#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
exec env RUST_LOG=info "${ROOT_DIR}/bin/surveillance_scanner" "${ROOT_DIR}/config/surveillance.toml"
