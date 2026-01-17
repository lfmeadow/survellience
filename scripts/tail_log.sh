#!/bin/bash
set -euo pipefail

exec sudo journalctl -u surveillance-collect -f
