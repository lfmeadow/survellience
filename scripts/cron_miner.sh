#!/bin/bash
# Cron job: Run miner at 11:59 PM UTC daily, then mm_viability report
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOG_DIR="${PROJECT_DIR}/logs"
REPORT_DIR="${PROJECT_DIR}/reports"
DATE=$(date -u +%Y-%m-%d)
TIMESTAMP=$(date -u +%Y-%m-%d_%H%M%S)

mkdir -p "${LOG_DIR}" "${REPORT_DIR}"

echo "[$(date -u)] Starting miner for ${DATE}" >> "${LOG_DIR}/cron.log"

cd "${PROJECT_DIR}"

# Run miner
"${PROJECT_DIR}/target/release/surveillance_miner" \
    --config "${PROJECT_DIR}/config/surveillance.toml" \
    analyze \
    --venue polymarket \
    --date "${DATE}" \
    >> "${LOG_DIR}/miner_${DATE}.log" 2>&1

echo "[$(date -u)] Miner completed, running mm_viability" >> "${LOG_DIR}/cron.log"

# Run mm_viability and save timestamped report
"${PROJECT_DIR}/target/release/surveillance_miner" \
    --config "${PROJECT_DIR}/config/surveillance.toml" \
    mm-viability \
    --venue polymarket \
    --date "${DATE}" \
    --hours all \
    --min-rows 100 \
    --top 50 \
    --fee-estimate 0.0 \
    --write-report true \
    > "${REPORT_DIR}/mm_viability_${TIMESTAMP}.txt" 2>&1

echo "[$(date -u)] MM viability report saved: mm_viability_${TIMESTAMP}.txt" >> "${LOG_DIR}/cron.log"

# Also create a symlink to latest report
ln -sf "mm_viability_${TIMESTAMP}.txt" "${REPORT_DIR}/mm_viability_latest.txt"

echo "[$(date -u)] All jobs completed" >> "${LOG_DIR}/cron.log"
