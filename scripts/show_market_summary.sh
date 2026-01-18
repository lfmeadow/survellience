#!/bin/bash
# Display summary of all markets in parquet files for a given day
# Shows row counts per market/outcome
# Uses virtual environment if available

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Helper function to get Python command (uses venv if available)
get_python_cmd() {
    local venv_python="${ROOT_DIR}/venv/bin/python3"
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

# Check if venv exists and warn if not
if [ ! -f "${ROOT_DIR}/venv/bin/python3" ]; then
    echo "Warning: Virtual environment not found at ${ROOT_DIR}/venv"
    echo "Using system python3. For best results, run: ./scripts/setup_python_env.sh"
    echo ""
fi

# Check if polars is available
if ! "$PYTHON_CMD" -c "import polars" 2>/dev/null; then
    echo "ERROR: polars not found."
    if [ -f "${ROOT_DIR}/venv/bin/python3" ]; then
        echo "Install with: ${ROOT_DIR}/venv/bin/pip install -r ${SCRIPT_DIR}/requirements.txt"
    else
        echo "Install with: pip install polars pyarrow"
    fi
    exit 1
fi

# Run the Python script
exec "$PYTHON_CMD" "${SCRIPT_DIR}/show_market_summary.py" "$@"
