#!/bin/bash
# LLM-based Rules Parser - Convert rules to symbolic logic
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
PROVIDER="${3:-anthropic}"
LIMIT="${4:-}"

# Get Python command
if [ -f "${PROJECT_DIR}/venv/bin/python3" ]; then
    PYTHON_CMD="${PROJECT_DIR}/venv/bin/python3"
else
    PYTHON_CMD="python3"
fi

# Check for API keys
if [ "$PROVIDER" = "anthropic" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "ERROR: ANTHROPIC_API_KEY not set"
    echo "  export ANTHROPIC_API_KEY=your-key"
    exit 1
fi

if [ "$PROVIDER" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "ERROR: OPENAI_API_KEY not set"
    echo "  export OPENAI_API_KEY=your-key"
    exit 1
fi

echo "LLM Rules Parser"
echo "  Venue: $VENUE"
echo "  Date: $DATE"
echo "  Provider: $PROVIDER"
[ -n "$LIMIT" ] && echo "  Limit: $LIMIT"
echo ""

ARGS=(
    --venue "$VENUE"
    --date "$DATE"
    --data-dir "${PROJECT_DIR}/data"
    --provider "$PROVIDER"
)

[ -n "$LIMIT" ] && ARGS+=(--limit "$LIMIT")

exec "$PYTHON_CMD" "${SCRIPT_DIR}/llm_rules_parser.py" "${ARGS[@]}"
