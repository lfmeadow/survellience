#!/bin/bash
# Setup Python virtual environment for surveillance system monitoring scripts

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${SCRIPT_DIR}/venv"
REQUIREMENTS_FILE="${SCRIPT_DIR}/requirements.txt"

echo "Setting up Python virtual environment..."

# Check if Python 3 is available
if ! command -v python3 > /dev/null 2>&1; then
    echo "Error: python3 is not installed. Please install Python 3 first."
    exit 1
fi

# Check if venv module is available
if ! python3 -m venv --help > /dev/null 2>&1; then
    echo "Error: python3 venv module is not available. Please install python3-venv package."
    echo "  On Ubuntu/Debian: sudo apt-get install python3-venv"
    exit 1
fi

# Create virtual environment if it doesn't exist
if [ ! -d "$VENV_DIR" ]; then
    echo "Creating virtual environment at $VENV_DIR..."
    python3 -m venv "$VENV_DIR"
    echo "Virtual environment created."
else
    echo "Virtual environment already exists at $VENV_DIR"
fi

# Activate virtual environment
echo "Activating virtual environment..."
source "${VENV_DIR}/bin/activate"

# Upgrade pip
echo "Upgrading pip..."
pip install --upgrade pip --quiet

# Install requirements
if [ -f "$REQUIREMENTS_FILE" ]; then
    echo "Installing requirements from $REQUIREMENTS_FILE..."
    pip install -r "$REQUIREMENTS_FILE" --quiet
    echo "Requirements installed successfully."
else
    echo "Warning: requirements.txt not found at $REQUIREMENTS_FILE"
    echo "Installing default packages..."
    pip install polars pyarrow --quiet
    echo "Default packages installed."
fi

echo ""
echo "✅ Python virtual environment setup complete!"
echo ""
echo "To activate the virtual environment, run:"
echo "  source venv/bin/activate"
echo ""
echo "To deactivate, run:"
echo "  deactivate"
