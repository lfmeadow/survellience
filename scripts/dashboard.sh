#!/bin/bash
# Wrapper script for dashboard.py - uses existing virtual environment

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENUE="${1:-polymarket}"
DATE="${2:-}"

# Helper function to get Python command (uses venv if available)
get_python_cmd() {
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local root_dir="$(dirname "$script_dir")"
    local venv_python="${root_dir}/venv/bin/python3"
    if [ -f "$venv_python" ]; then
        echo "$venv_python"
    elif command -v python3 > /dev/null 2>&1; then
        echo "python3"
    else
        echo ""
    fi
}

# Get Python command (prefers venv)
PYTHON_CMD=$(get_python_cmd)

if [ -z "$PYTHON_CMD" ]; then
    echo "ERROR: python3 not found. Please install Python 3."
    exit 1
fi

# Check if polars is installed
if ! "$PYTHON_CMD" -c "import polars" 2>/dev/null; then
    echo "ERROR: polars not installed."
    local root_dir="$(dirname "$SCRIPT_DIR")"
    if [ -f "${root_dir}/venv/bin/python3" ]; then
        echo "Virtual environment found but polars is missing."
        echo "Install with: ${root_dir}/venv/bin/pip install -r ${SCRIPT_DIR}/requirements.txt"
    else
        echo "Install with: pip install polars"
    fi
    exit 1
fi

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    cat <<EOF
Usage: $0 [venue] [YYYY-MM-DD] [--refresh N]

Examples:
  $0 polymarket
  $0 polymarket 2026-01-17 --refresh 3
EOF
    exit 0
fi

# Run dashboard (HTML)
cd "$SCRIPT_DIR"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
if [ -n "$DATE" ]; then
    "$PYTHON_CMD" "${SCRIPT_DIR}/dashboard_web.py" "$VENUE" --date "$DATE" --data-dir "${ROOT_DIR}/data" "${@:3}"
else
    "$PYTHON_CMD" "${SCRIPT_DIR}/dashboard_web.py" "$VENUE" --data-dir "${ROOT_DIR}/data" "${@:2}"
fi
