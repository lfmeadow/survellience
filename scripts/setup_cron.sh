#!/bin/bash
# Set up cron jobs for surveillance system
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "Setting up cron jobs for surveillance system..."
echo "Project directory: ${PROJECT_DIR}"

# Make scripts executable
chmod +x "${SCRIPT_DIR}/cron_scanner.sh"
chmod +x "${SCRIPT_DIR}/cron_miner.sh"

# Create cron entries
CRON_SCANNER="1 0 * * * ${SCRIPT_DIR}/cron_scanner.sh"
CRON_MINER="59 23 * * * ${SCRIPT_DIR}/cron_miner.sh"

# Check if entries already exist
EXISTING_CRON=$(crontab -l 2>/dev/null || true)

if echo "${EXISTING_CRON}" | grep -q "cron_scanner.sh"; then
    echo "Scanner cron job already exists"
else
    echo "Adding scanner cron job (12:01 AM UTC)"
fi

if echo "${EXISTING_CRON}" | grep -q "cron_miner.sh"; then
    echo "Miner cron job already exists"
else
    echo "Adding miner cron job (11:59 PM UTC)"
fi

# Install cron jobs (preserving existing entries)
(
    echo "${EXISTING_CRON}" | grep -v "cron_scanner.sh" | grep -v "cron_miner.sh" || true
    echo ""
    echo "# Surveillance system cron jobs (UTC times)"
    echo "${CRON_SCANNER}"
    echo "${CRON_MINER}"
) | crontab -

echo ""
echo "Cron jobs installed:"
echo "  - Scanner: 12:01 AM UTC daily"
echo "  - Miner + MM Viability: 11:59 PM UTC daily"
echo ""
echo "View cron jobs with: crontab -l"
echo "View logs in: ${PROJECT_DIR}/logs/"
echo "View reports in: ${PROJECT_DIR}/reports/"
echo ""
echo "To remove cron jobs, run: crontab -e"
