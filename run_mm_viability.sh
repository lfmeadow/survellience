#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
HOURS="${3:-all}"
MIN_ROWS="${4:-1000}"
TOP="${5:-20}"
FEE_ESTIMATE="${6:-0.0}"
WRITE_REPORT="${7:-true}"

exec "${SCRIPT_DIR}/target/release/surveillance_miner" \
  --config "${SCRIPT_DIR}/config/surveillance.toml" \
  mm-viability \
  --venue "$VENUE" \
  --date "$DATE" \
  --hours "$HOURS" \
  --min-rows "$MIN_ROWS" \
  --top "$TOP" \
  --fee-estimate "$FEE_ESTIMATE" \
  --write-report "$WRITE_REPORT"
