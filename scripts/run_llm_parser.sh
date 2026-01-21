#!/bin/bash
# LLM-based Rules Parser - Convert rules to symbolic logic
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Load API keys from .env
if [ -f "${PROJECT_DIR}/.env" ]; then
    set -a
    source "${PROJECT_DIR}/.env"
    set +a
fi

VENUE="${1:-polymarket}"
DATE="${2:-$(date -u +%Y-%m-%d)}"
PROVIDER="${3:-ollama}"
MODEL="${4:-qwen2.5:7b}"
LIMIT="${5:-}"

# Get Python command
if [ -f "${PROJECT_DIR}/venv/bin/python3" ]; then
    PYTHON_CMD="${PROJECT_DIR}/venv/bin/python3"
else
    PYTHON_CMD="python3"
fi

# Check for API keys
if [ "$PROVIDER" = "anthropic" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "ERROR: ANTHROPIC_API_KEY not set"
    exit 1
fi

if [ "$PROVIDER" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "ERROR: OPENAI_API_KEY not set"
    exit 1
fi

if [ "$PROVIDER" = "ollama" ]; then
    if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo "ERROR: Ollama not running. Start with: ollama serve"
        exit 1
    fi
fi

echo "LLM Rules Parser"
echo "  Venue: $VENUE"
echo "  Date: $DATE"
echo "  Provider: $PROVIDER"
echo "  Model: $MODEL"
[ -n "$LIMIT" ] && echo "  Limit: $LIMIT"
echo ""

ARGS=(
    --venue "$VENUE"
    --date "$DATE"
    --data-dir "${PROJECT_DIR}/data"
    --provider "$PROVIDER"
    --model "$MODEL"
)

[ -n "$LIMIT" ] && ARGS+=(--limit "$LIMIT")

exec "$PYTHON_CMD" "${SCRIPT_DIR}/llm_rules_parser.py" "${ARGS[@]}"
