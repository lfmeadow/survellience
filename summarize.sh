#!/bin/bash
# Generate comprehensive summary of collected data

set -e

VENUE="${1:-polymarket}"
DATE="${2:-$(date +%Y-%m-%d)}"

echo "=========================================="
echo "  Data Summary for $VENUE - $DATE"
echo "=========================================="
echo ""

# Run the miner to generate/update stats
echo "=== Generating Statistics ==="
if [ -f "./target/release/surveillance_miner" ]; then
    ./target/release/surveillance_miner config/surveillance.toml "$VENUE" "$DATE" 2>&1 | grep -v "^[0-9]" | head -50
else
    echo "⚠️  Miner binary not found. Build with: cargo build --release"
fi
echo ""

# Show stats cache contents
STATS_FILE="data/stats/venue=$VENUE/date=$DATE/stats.csv"
if [ -f "$STATS_FILE" ]; then
    echo "=== Statistics Cache Contents ==="
    if command -v column > /dev/null; then
        head -20 "$STATS_FILE" | column -t -s','
    else
        head -20 "$STATS_FILE"
    fi
    
    TOTAL=$(tail -n +2 "$STATS_FILE" 2>/dev/null | wc -l)
    if [ "$TOTAL" -gt 20 ]; then
        echo "... ($TOTAL total rows)"
    fi
    echo ""
fi

# Show file listing
echo "=== Data Files ==="
SNAPSHOT_DIR="data/orderbook_snapshots/venue=$VENUE/date=$DATE"
if [ -d "$SNAPSHOT_DIR" ]; then
    echo "Parquet files:"
    find "$SNAPSHOT_DIR" -name "*.parquet" -type f -exec ls -lh {} \; | \
        awk '{print "  " $9 " (" $5 ")"}' | \
        sed "s|$SNAPSHOT_DIR/||"
else
    echo "No snapshot directory found"
fi
echo ""

echo "=========================================="
echo "For detailed monitoring, run: ./monitor.sh $VENUE $DATE"
echo "=========================================="
