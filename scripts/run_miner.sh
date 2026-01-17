#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"

exec "${SCRIPT_DIR}/target/release/surveillance_miner" \
  --config "${SCRIPT_DIR}/config/surveillance.toml" \
  mine \
  --venue "$VENUE" \
  --date "$DATE"
