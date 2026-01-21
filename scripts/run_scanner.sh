#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

exec env RUST_LOG=info "${PROJECT_DIR}/target/release/surveillance_scanner" "${PROJECT_DIR}/config/surveillance.toml"
