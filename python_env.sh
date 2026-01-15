#!/bin/bash
# Source this file to activate the Python virtual environment
# Usage: source python_env.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${SCRIPT_DIR}/venv"

if [ -d "${VENV_DIR}/bin" ]; then
    source "${VENV_DIR}/bin/activate"
else
    echo "Warning: Virtual environment not found at ${VENV_DIR}"
    echo "Run ./setup_python_env.sh to create it."
    return 1 2>/dev/null || exit 1
fi
