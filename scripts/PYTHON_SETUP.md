# Python Environment Setup

## Overview

The surveillance system monitoring scripts use Python (with Polars and PyArrow) for data analysis. To avoid dependency conflicts and ensure proper package management, Python packages should be installed in a virtual environment.

## Quick Setup

Run the setup script to create and configure the virtual environment:

```bash
./setup_python_env.sh
```

This will:
1. Create a Python virtual environment in `venv/`
2. Install required packages (polars, pyarrow) from `requirements.txt`
3. Show instructions for activation

## Manual Setup

If you prefer to set it up manually:

```bash
# Create virtual environment
python3 -m venv venv

# Activate virtual environment
source venv/bin/activate

# Install requirements
pip install -r requirements.txt

# Or install individually
pip install polars pyarrow
```

## Using the Virtual Environment

### Activate the Virtual Environment

```bash
source venv/bin/activate
```

Or use the helper script:
```bash
source python_env.sh
```

### Deactivate

```bash
deactivate
```

## Requirements

The monitoring scripts require:
- **polars** (>= 0.20.0): For reading and analyzing Parquet files
- **pyarrow** (>= 12.0.0): For Parquet file operations (optional, but recommended)

These are listed in `requirements.txt`.

## Automatic Detection

The monitoring scripts (`monitor.sh` and `monitor_parquet.sh`) automatically detect and use the virtual environment if it exists. If the venv is not found, they fall back to using the system `python3` (if available).

**No manual activation needed** - the scripts handle it automatically!

## Troubleshooting

### Python 3 Not Found

Install Python 3:
```bash
# Ubuntu/Debian
sudo apt-get install python3 python3-pip python3-venv

# Fedora/RHEL
sudo dnf install python3 python3-pip

# macOS (with Homebrew)
brew install python3
```

### venv Module Not Available

Install the venv module:
```bash
# Ubuntu/Debian
sudo apt-get install python3-venv

# Fedora/RHEL
sudo dnf install python3-venv
```

### Permission Errors

If you get permission errors when installing packages, make sure the virtual environment is activated:
```bash
source venv/bin/activate
which python3  # Should show: /path/to/survellience/venv/bin/python3
```

### Polars Import Error

If scripts say "Polars not available" even after setup:
1. Verify the virtual environment exists: `ls -la venv/bin/python3`
2. Reinstall: `source venv/bin/activate && pip install --upgrade polars`
3. Check installation: `python3 -c "import polars; print(polars.__version__)"`

## Scripts That Use Python

- **monitor.sh**: Uses Polars to analyze Parquet files and show data summaries
- **monitor_parquet.sh**: Uses Python for file size analysis

Both scripts automatically detect and use the virtual environment if it exists.

## Verification

Test the setup:

```bash
# Setup (if not done already)
./setup_python_env.sh

# Verify packages are installed
venv/bin/python3 -c "import polars; print('Polars:', polars.__version__)"
venv/bin/python3 -c "import pyarrow; print('PyArrow:', pyarrow.__version__)"

# Run a monitoring script (should use venv automatically)
./monitor.sh polymarket
```

## Updating Packages

To update packages in the virtual environment:

```bash
source venv/bin/activate
pip install --upgrade polars pyarrow
```

Or update all packages:
```bash
source venv/bin/activate
pip install --upgrade -r requirements.txt
```

## Removing the Virtual Environment

To remove the virtual environment:

```bash
rm -rf venv/
```

Then re-run `./setup_python_env.sh` if needed.
