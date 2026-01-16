#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

exec "${SCRIPT_DIR}/target/release/surveillance_miner" \
  "${SCRIPT_DIR}/config/surveillance.toml" \
  "$@"
