#!/bin/bash
# Cron job: Run scanner at 12:01 AM UTC daily
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOG_DIR="${PROJECT_DIR}/logs"
DATE=$(date -u +%Y-%m-%d)

mkdir -p "${LOG_DIR}"

echo "[$(date -u)] Starting scanner for ${DATE}" >> "${LOG_DIR}/cron.log"

cd "${PROJECT_DIR}"
"${PROJECT_DIR}/target/release/surveillance_scanner" \
    --config "${PROJECT_DIR}/config/surveillance.toml" \
    --venue polymarket \
    >> "${LOG_DIR}/scanner_${DATE}.log" 2>&1

echo "[$(date -u)] Scanner completed" >> "${LOG_DIR}/cron.log"
