#!/bin/bash
# Comprehensive monitoring and summarization script for surveillance system

set -e

VENUE="${1:-polymarket}"
# Default to UTC date to match file storage (files are stored with UTC dates)
DATE="${2:-$(date -u +%Y-%m-%d)}"

echo "=========================================="
echo "  Surveillance System Monitor & Summary"
echo "=========================================="
echo "Venue: $VENUE"
echo "Date: $DATE"
echo ""

# 1. System Health
echo "=== SYSTEM HEALTH ==="
if command -v systemctl > /dev/null 2>&1 && systemctl is-active --quiet surveillance-collect 2>/dev/null; then
    PID=$(systemctl show -p MainPID --value surveillance-collect 2>/dev/null)
    echo "✅ Collector: RUNNING (systemd service)"
    echo "   PID: $PID"
    if command -v ps > /dev/null; then
        MEM=$(ps -p "$PID" -o rss= 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
        echo "   Memory: $MEM"
    fi
elif pgrep -f surveillance_collect > /dev/null; then
    PID=$(pgrep -f surveillance_collect | head -1)
    echo "✅ Collector: RUNNING (direct process)"
    echo "   PID: $PID"
    if command -v ps > /dev/null; then
        MEM=$(ps -p "$PID" -o rss= 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
        echo "   Memory: $MEM"
    fi
else
    echo "❌ Collector: NOT RUNNING"
fi
echo ""

# 2. Data Collection Status
echo "=== DATA COLLECTION ==="
SNAPSHOT_DIR="data/orderbook_snapshots/venue=$VENUE/date=$DATE"
if [ -d "$SNAPSHOT_DIR" ]; then
    # Count files by hour
    echo "Parquet files by hour:"
    for hour_dir in "$SNAPSHOT_DIR"/hour=*; do
        if [ -d "$hour_dir" ]; then
            hour=$(basename "$hour_dir" | sed 's/hour=//')
            count=$(find "$hour_dir" -name "*.parquet" -type f 2>/dev/null | wc -l)
            if [ "$count" -gt 0 ]; then
                size=$(du -sh "$hour_dir" 2>/dev/null | cut -f1)
                echo "  Hour $hour: $count files ($size)"
            fi
        fi
    done
    
    # Total stats
    TOTAL_FILES=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f 2>/dev/null | wc -l)
    TOTAL_SIZE=$(du -sh "$SNAPSHOT_DIR" 2>/dev/null | cut -f1)
    echo ""
    echo "Total: $TOTAL_FILES files, $TOTAL_SIZE"
    
    # Recent activity (last 10 minutes)
    RECENT=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f -mmin -10 2>/dev/null | wc -l)
    if [ "$RECENT" -gt 0 ]; then
        echo "✅ Recent activity: $RECENT files in last 10 minutes"
    else
        echo "⚠️  No recent files (last 10 minutes)"
    fi
else
    echo "⚠️  No snapshot directory found for $VENUE/$DATE"
fi
echo ""

# 3.5 Trade Data (if available)
echo "=== TRADE DATA ==="
TRADE_DIR="data/trades/venue=$VENUE/date=$DATE"
if [ -d "$TRADE_DIR" ]; then
    TRADE_FILES=$(find "$TRADE_DIR" -name "*.parquet" -type f 2>/dev/null | wc -l)
    if [ "$TRADE_FILES" -gt 0 ]; then
        TOTAL_TRADE_SIZE=$(du -sh "$TRADE_DIR" 2>/dev/null | cut -f1)
        echo "Trade parquet files: $TRADE_FILES ($TOTAL_TRADE_SIZE)"
        LAST_TRADE_FILE=$(find "$TRADE_DIR" -name "*.parquet" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)
        if [ -n "$LAST_TRADE_FILE" ]; then
            echo "Latest trade file: ${LAST_TRADE_FILE#${TRADE_DIR}/}"
        fi
    else
        echo "No trade parquet files found"
    fi
else
    echo "No trade directory found"
fi
echo ""

# 3. Market Universe
echo "=== MARKET UNIVERSE ==="
# Use UTC date for universe file to match file storage
UNIVERSE_DATE="${DATE:-$(date -u +%Y-%m-%d)}"
UNIVERSE_FILE="data/metadata/venue=$VENUE/date=$UNIVERSE_DATE/universe.jsonl"
if [ -f "$UNIVERSE_FILE" ]; then
    MARKET_COUNT=$(wc -l < "$UNIVERSE_FILE" 2>/dev/null || echo "0")
    echo "Markets discovered: $MARKET_COUNT"
    
    # Count markets with token IDs
    if command -v jq > /dev/null; then
        WITH_TOKENS=$(grep -c '"token_ids":\[' "$UNIVERSE_FILE" 2>/dev/null || echo "0")
        echo "Markets with token IDs: $WITH_TOKENS"
    fi
else
    echo "⚠️  Universe file not found: $UNIVERSE_FILE"
    echo "   Run: ./target/release/surveillance_scanner config/surveillance.toml"
fi
echo ""

# 4. Statistics Cache
echo "=== STATISTICS CACHE ==="
STATS_DIR="data/stats/venue=$VENUE/date=$UNIVERSE_DATE"
if [ -f "$STATS_DIR/stats.csv" ] || [ -f "$STATS_DIR/stats.parquet" ]; then
    if [ -f "$STATS_DIR/stats.csv" ]; then
        STATS_FILE="$STATS_DIR/stats.csv"
        ROW_COUNT=$(tail -n +2 "$STATS_FILE" 2>/dev/null | wc -l || echo "0")
        echo "✅ Stats cache exists (CSV): $ROW_COUNT market/outcome pairs"
        
        if [ "$ROW_COUNT" -gt 0 ] && command -v awk > /dev/null; then
            echo ""
            echo "Summary statistics:"
            tail -n +2 "$STATS_FILE" 2>/dev/null | awk -F',' '
            BEGIN {
                total_depth=0; total_spread=0; total_updates=0; count=0
            }
            {
                if ($3 != "" && $3 != "avg_depth") {
                    total_depth += $3
                    total_spread += $4
                    total_updates += $5
                    count++
                }
            }
            END {
                if (count > 0) {
                    printf "  Avg depth: %.2f\n", total_depth/count
                    printf "  Avg spread: %.6f\n", total_spread/count
                    printf "  Total updates: %.0f\n", total_updates
                    printf "  Avg updates per market: %.1f\n", total_updates/count
                }
            }'
        fi
    elif [ -f "$STATS_DIR/stats.parquet" ]; then
        echo "✅ Stats cache exists (Parquet)"
        echo "   Run miner to view details"
    fi
else
    echo "⚠️  No stats cache found for $VENUE/$DATE"
    echo "   Run: ./target/release/surveillance_miner --config config/surveillance.toml mine --venue $VENUE --date $DATE"
fi
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

# 5. Quick Data Summary (if Polars/Python available)
PYTHON_CMD=$(get_python_cmd)
if [ -n "$PYTHON_CMD" ]; then
    echo "=== QUICK DATA SUMMARY ==="
    if [ -d "$SNAPSHOT_DIR" ] && [ "$TOTAL_FILES" -gt 0 ]; then
        "$PYTHON_CMD" << EOF
import sys
try:
    import polars as pl
    from pathlib import Path
    
    snapshot_dir = Path("$SNAPSHOT_DIR")
    dfs = []
    
    for hour_dir in snapshot_dir.glob("hour=*"):
        for parquet_file in hour_dir.glob("*.parquet"):
            try:
                df = pl.read_parquet(parquet_file)
                dfs.append(df)
            except Exception as e:
                pass
    
    if dfs:
        combined = pl.concat(dfs)
        print(f"Total snapshots: {len(combined):,}")
        print(f"Unique markets: {combined['market_id'].n_unique()}")
        print(f"Unique outcomes: {combined.select(['market_id', 'outcome_id']).unique().height}")
        print(f"Time range: {combined['ts_recv'].min()} to {combined['ts_recv'].max()}")
        
        # Top markets by update count
        top_markets = combined.group_by(['market_id', 'outcome_id']).agg([
            pl.len().alias('updates'),
            pl.mean('spread').alias('avg_spread'),
            (pl.mean('best_bid_sz') + pl.mean('best_ask_sz')).alias('avg_depth')
        ]).sort('updates', descending=True).head(5)
        
        if len(top_markets) > 0:
            print("\nTop 5 markets by update count:")
            print(top_markets)
    else:
        print("No data files found")
except ImportError:
    print("Polars not available - install with: pip install polars")
except Exception as e:
    print(f"Error: {e}")
EOF
    else
        echo "No snapshot data available"
    fi
    echo ""
fi

# 6. Disk Usage
echo "=== DISK USAGE ==="
if [ -d "data" ]; then
    echo "Data directory breakdown:"
    du -sh data/* 2>/dev/null | sort -h | sed 's/^/  /'
    TOTAL=$(du -sh data 2>/dev/null | cut -f1)
    echo "  Total: $TOTAL"
fi
echo ""

# 7. Recent Logs (if available)
echo "=== RECENT ACTIVITY ==="

# Check if running via systemd (preferred)
if command -v systemctl > /dev/null 2>&1 && systemctl is-active --quiet surveillance-collect 2>/dev/null; then
    # Show last 5 log lines from systemd journal
    if command -v journalctl > /dev/null 2>&1; then
        echo "Last 5 log lines (systemd journal):"
        sudo journalctl -u surveillance-collect -n 5 --no-pager 2>/dev/null | tail -5 | sed 's/^/  /'
        
        # Error summary from journal
        ERROR_COUNT=$(sudo journalctl -u surveillance-collect -n 100 --no-pager 2>/dev/null | grep -E " (WARN|ERROR|FATAL) " | wc -l)
        if [ "$ERROR_COUNT" -gt 0 ]; then
            echo ""
            echo "⚠️  Recent errors/warnings in journal: $ERROR_COUNT (last 100 lines)"
            sudo journalctl -u surveillance-collect -n 100 --no-pager 2>/dev/null | grep -E " (WARN|ERROR|FATAL) " | tail -3 | sed 's/^/  /'
        fi

        # Activity summary from journal
        SUB_COUNT=$(sudo journalctl -u surveillance-collect -n 200 --no-pager 2>/dev/null | grep -iE "subscription update|subscribing to" | wc -l)
        ROT_COUNT=$(sudo journalctl -u surveillance-collect -n 200 --no-pager 2>/dev/null | grep -iE "rotating subscriptions" | wc -l)
        WRITE_COUNT=$(sudo journalctl -u surveillance-collect -n 200 --no-pager 2>/dev/null | grep -iE "Wrote .*snapshots_.*parquet" | wc -l)
        echo ""
        echo "Activity (last 200 lines): subscriptions=$SUB_COUNT, rotations=$ROT_COUNT, writes=$WRITE_COUNT"
        LAST_METRICS=$(sudo journalctl -u surveillance-collect -n 200 --no-pager 2>/dev/null | grep "WebSocket metrics" | tail -1)
        if [ -n "$LAST_METRICS" ]; then
            echo "Latest WebSocket metrics: ${LAST_METRICS#*: }"
        fi
        LAST_CURSOR=$(sudo journalctl -u surveillance-collect -n 200 --no-pager 2>/dev/null | grep "WARM cursor start" | tail -1)
        if [ -n "$LAST_CURSOR" ]; then
            echo "Latest WARM cursor: ${LAST_CURSOR#*: }"
        fi
    else
        echo "journalctl not available"
    fi
elif [ -f "collector.log" ] && [ -s "collector.log" ]; then
    # Fallback: check collector.log if it exists and has content
    echo "Last 5 log lines (collector.log):"
    tail -5 collector.log 2>/dev/null | sed 's/^/  /'
    
    # Error summary
    ERROR_COUNT=$(tail -100 collector.log 2>/dev/null | grep -iE "error|failed" | wc -l)
    if [ "$ERROR_COUNT" -gt 0 ]; then
        echo ""
        echo "⚠️  Recent errors in logs: $ERROR_COUNT (last 100 lines)"
    fi
else
    echo "No recent activity logs found"
    if pgrep -f surveillance_collect > /dev/null || systemctl is-active --quiet surveillance-collect 2>/dev/null; then
        echo "  (Collector is running - logs may be in systemd journal)"
        echo "  View with: sudo journalctl -u surveillance-collect -f"
    fi
fi
echo ""

# 8. Recommendations
echo "=== RECOMMENDATIONS ==="
if [ ! -f "$UNIVERSE_FILE" ]; then
    echo "⚠️  Run scanner to discover markets:"
    echo "   ./target/release/surveillance_scanner config/surveillance.toml"
fi

if [ "$TOTAL_FILES" -eq 0 ]; then
    echo "⚠️  No data files found - check collector is running"
fi

if [ ! -f "$STATS_DIR/stats.csv" ] && [ ! -f "$STATS_DIR/stats.parquet" ]; then
    echo "💡 Generate statistics:"
    echo "   ./target/release/surveillance_miner config/surveillance.toml $VENUE $UNIVERSE_DATE"
fi

# Only warn about no recent files if collector is running AND we checked the correct date directory
if [ "$RECENT" -eq 0 ] && [ -d "$SNAPSHOT_DIR" ]; then
    if pgrep -f surveillance_collect > /dev/null || systemctl is-active --quiet surveillance-collect 2>/dev/null; then
        LAST_FILE=$(find "$SNAPSHOT_DIR" -name "*.parquet" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)
        if [ -n "$LAST_FILE" ]; then
            FILE_AGE_MIN=$((($(date +%s) - $(stat -c %Y "$LAST_FILE" 2>/dev/null || echo 0)) / 60))
            echo "⚠️  Collector running but no recent files (last file: $FILE_AGE_MIN minutes ago) - check WebSocket connection"
        else
            echo "⚠️  Collector running but no files found - check WebSocket connection"
        fi
    fi
fi

echo ""
echo "=========================================="
