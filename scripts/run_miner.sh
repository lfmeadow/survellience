#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"

exec "${ROOT_DIR}/bin/surveillance_miner" \
  --config "${ROOT_DIR}/config/surveillance.toml" \
  mine \
  --venue "$VENUE" \
  --date "$DATE"
