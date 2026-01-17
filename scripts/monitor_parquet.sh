#!/bin/bash
# Monitor Parquet files for surveillance system

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
VENUE="${1:-polymarket}"
DATE="${2:-$(date +%Y-%m-%d)}"
SNAPSHOT_DIR="${ROOT_DIR}/data/orderbook_snapshots/venue=$VENUE/date=$DATE"

echo "=========================================="
echo "  Parquet File Monitor"
echo "=========================================="
echo "Venue: $VENUE"
echo "Date: $DATE"
echo ""

if [ ! -d "$SNAPSHOT_DIR" ]; then
    echo "⚠️  No snapshot directory found: $SNAPSHOT_DIR"
    exit 1
fi

# Count files by hour
echo "=== FILES BY HOUR ==="
for hour_dir in "$SNAPSHOT_DIR"/hour=*; do
    if [ -d "$hour_dir" ]; then
        hour=$(basename "$hour_dir" | sed 's/hour=//')
        count=$(find "$hour_dir" -name "*.parquet" -type f 2>/dev/null | wc -l)
        if [ "$count" -gt 0 ]; then
            size=$(du -sh "$hour_dir" 2>/dev/null | cut -f1)
            latest=$(find "$hour_dir" -name "*.parquet" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2- | xargs basename 2>/dev/null || echo "N/A")
            echo "  Hour $hour: $count files ($size) [Latest: $latest]"
        fi
    fi
done
echo ""

# Total stats
TOTAL_FILES=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f 2>/dev/null | wc -l)
TOTAL_SIZE=$(du -sh "$SNAPSHOT_DIR" 2>/dev/null | cut -f1)
echo "=== TOTAL STATS ==="
echo "Total files: $TOTAL_FILES"
echo "Total size: $TOTAL_SIZE"
echo ""

# Recent activity
echo "=== RECENT ACTIVITY ==="
RECENT_5M=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f -mmin -5 2>/dev/null | wc -l)
RECENT_10M=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f -mmin -10 2>/dev/null | wc -l)
echo "Files created in last 5 minutes: $RECENT_5M"
echo "Files created in last 10 minutes: $RECENT_10M"
echo ""

# Latest files
echo "=== LATEST FILES ==="
find "$SNAPSHOT_DIR" -name "*.parquet" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -5 | while read timestamp filepath; do
    filename=$(basename "$filepath")
    size=$(du -h "$filepath" 2>/dev/null | cut -f1)
    mod_time=$(date -d "@$timestamp" '+%H:%M:%S' 2>/dev/null || echo "N/A")
    echo "  $mod_time - $filename ($size)"
done
echo ""

# Helper function to get Python command (uses venv if available)
get_python_cmd() {
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local venv_python="${script_dir}/venv/bin/python3"
    if [ -f "$venv_python" ]; then
        echo "$venv_python"
    elif command -v python3 > /dev/null 2>&1; then
        echo "python3"
    else
        echo ""
    fi
}

# File size distribution
echo "=== FILE SIZE DISTRIBUTION ==="
PYTHON_CMD=$(get_python_cmd)
if [ -n "$PYTHON_CMD" ]; then
    "$PYTHON_CMD" << PYEOF
import os
from pathlib import Path
from collections import defaultdict

snapshot_dir = Path("$SNAPSHOT_DIR")
sizes = []
for parquet_file in snapshot_dir.rglob("*.parquet"):
    try:
        size = parquet_file.stat().st_size
        sizes.append(size)
    except:
        pass

if sizes:
    sizes.sort()
    total = sum(sizes)
    count = len(sizes)
    avg = total / count if count > 0 else 0
    
    print(f"  Count: {count}")
    print(f"  Total: {total / 1024 / 1024:.2f} MB")
    print(f"  Average: {avg / 1024:.2f} KB")
    print(f"  Min: {min(sizes) / 1024:.2f} KB")
    print(f"  Max: {max(sizes) / 1024 / 1024:.2f} MB")
    print(f"  Median: {sizes[len(sizes)//2] / 1024:.2f} KB")
else:
    print("  No files found")
PYEOF
else
    echo "  (Python3 not available for size analysis)"
fi
echo ""

# Watch mode
if [ "$3" == "--watch" ]; then
    echo "=== WATCH MODE (Ctrl+C to exit) ==="
    while true; do
        clear
        "$0" "$VENUE" "$DATE"
        sleep 5
    done
fi
