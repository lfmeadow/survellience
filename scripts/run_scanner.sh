#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec env RUST_LOG=info "${SCRIPT_DIR}/target/release/surveillance_scanner" "${SCRIPT_DIR}/config/surveillance.toml"
