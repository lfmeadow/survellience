#!/bin/bash
# Ingest rules from venue APIs
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
LIMIT="${3:-}"
FORCE="${4:-}"

ARGS="--venue ${VENUE} --date ${DATE}"

if [[ -n "${LIMIT}" ]]; then
    ARGS="${ARGS} --limit ${LIMIT}"
fi

if [[ "${FORCE}" == "--force" || "${FORCE}" == "true" ]]; then
    ARGS="${ARGS} --force"
fi

echo "Ingesting rules for venue=${VENUE}, date=${DATE}"
exec "${PROJECT_DIR}/target/release/surveillance_rules" ingest ${ARGS}
