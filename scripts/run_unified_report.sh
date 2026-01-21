#!/bin/bash
# Generate unified report with all static and dynamic info per token-id
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
FORMAT="${3:-table}"

# Get Python command (prefers venv)
if [ -f "${PROJECT_DIR}/venv/bin/python3" ]; then
    PYTHON_CMD="${PROJECT_DIR}/venv/bin/python3"
elif command -v python3 > /dev/null 2>&1; then
    PYTHON_CMD="python3"
else
    echo "ERROR: python3 not found"
    exit 1
fi

exec "$PYTHON_CMD" "${SCRIPT_DIR}/unified_report.py" \
    --venue "$VENUE" \
    --date "$DATE" \
    --data-dir "${PROJECT_DIR}/data" \
    --format "$FORMAT"
