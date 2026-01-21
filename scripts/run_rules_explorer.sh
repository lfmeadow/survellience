#!/bin/bash
# Interactive Rules Explorer - Visual browser for rules pipeline data
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
PORT="${3:-8082}"

# Get Python command (prefers venv)
if [ -f "${PROJECT_DIR}/venv/bin/python3" ]; then
    PYTHON_CMD="${PROJECT_DIR}/venv/bin/python3"
elif command -v python3 > /dev/null 2>&1; then
    PYTHON_CMD="python3"
else
    echo "ERROR: python3 not found"
    exit 1
fi

echo "Starting Rules Explorer..."
echo "  URL: http://localhost:${PORT}"
echo "  Venue: ${VENUE}"
echo "  Date: ${DATE}"
echo ""

exec "$PYTHON_CMD" "${SCRIPT_DIR}/rules_explorer.py" \
    --venue "$VENUE" \
    --date "$DATE" \
    --data-dir "${PROJECT_DIR}/data" \
    --port "$PORT"
