#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"

exec "${PROJECT_DIR}/target/release/surveillance_miner" \
  --config "${PROJECT_DIR}/config/surveillance.toml" \
  analyze \
  --venue "$VENUE" \
  --date "$DATE"
