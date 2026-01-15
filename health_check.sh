#!/bin/bash
# Health check script for surveillance system
# Updated to support systemd service and current system state

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR" || exit 1

echo "=== Surveillance System Health Check ==="
echo ""

# Check if collector is running (systemd service or direct process)
COLLECTOR_RUNNING=false
COLLECTOR_PID=""
COLLECTOR_METHOD=""

# Check systemd service first (preferred)
if systemctl is-active --quiet surveillance-collect 2>/dev/null; then
    COLLECTOR_RUNNING=true
    COLLECTOR_METHOD="systemd"
    COLLECTOR_PID=$(systemctl show -p MainPID --value surveillance-collect 2>/dev/null)
    echo "✅ Collector: RUNNING (systemd service)"
    echo "   Service: surveillance-collect.service"
    echo "   PID: $COLLECTOR_PID"
elif pgrep -f "surveillance_collect.*config" > /dev/null 2>&1; then
    # Fallback: check for direct process
    COLLECTOR_PID=$(pgrep -f "surveillance_collect.*config" | head -1)
    COLLECTOR_RUNNING=true
    COLLECTOR_METHOD="process"
    echo "✅ Collector: RUNNING (direct process)"
    echo "   PID: $COLLECTOR_PID"
else
    echo "❌ Collector: NOT RUNNING"
fi

# Get process details if running
if [ "$COLLECTOR_RUNNING" = true ] && [ -n "$COLLECTOR_PID" ]; then
    if ps -p "$COLLECTOR_PID" > /dev/null 2>&1; then
        # Memory usage
        MEM=$(ps -p "$COLLECTOR_PID" -o rss= 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
        [ -n "$MEM" ] && echo "   Memory: $MEM"
        
        # CPU usage
        CPU=$(ps -p "$COLLECTOR_PID" -o %cpu= 2>/dev/null | awk '{print $1"%"}')
        [ -n "$CPU" ] && echo "   CPU: $CPU"
        
        # Runtime
        RUNTIME=$(ps -p "$COLLECTOR_PID" -o etime= 2>/dev/null | awk '{print $1}')
        [ -n "$RUNTIME" ] && echo "   Runtime: $RUNTIME"
    fi
fi
echo ""

# Check recent Parquet files
echo "=== DATA COLLECTION ==="
RECENT_FILES=$(find data/orderbook_snapshots -name "*.parquet" -type f -mmin -10 2>/dev/null | wc -l)
if [ "$RECENT_FILES" -gt 0 ]; then
    echo "✅ Recent data files: $RECENT_FILES files in last 10 minutes"
    
    # Check most recent file
    MOST_RECENT=$(find data/orderbook_snapshots -name "*.parquet" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)
    if [ -n "$MOST_RECENT" ]; then
        FILE_AGE=$(find "$MOST_RECENT" -printf '%T@' 2>/dev/null)
        CURRENT_TIME=$(date +%s)
        AGE_SECONDS=$((CURRENT_TIME - ${FILE_AGE%.*}))
        AGE_MINUTES=$((AGE_SECONDS / 60))
        echo "   Most recent file: $(basename "$MOST_RECENT")"
        echo "   Age: $AGE_MINUTES minutes ago"
    fi
else
    echo "⚠️  No recent data files (last 10 minutes)"
    if [ "$COLLECTOR_RUNNING" = true ]; then
        echo "   ⚠️  Collector is running but no files - check WebSocket connection"
    fi
fi

# Disk space and file counts
if [ -d data/orderbook_snapshots ]; then
    DISK_USAGE=$(du -sh data/orderbook_snapshots 2>/dev/null | cut -f1)
    TOTAL_FILES=$(find data/orderbook_snapshots -name "*.parquet" -type f 2>/dev/null | wc -l)
    TODAY_FILES=$(find data/orderbook_snapshots -name "*.parquet" -type f -newermt "$(date +%Y-%m-%d)" 2>/dev/null | wc -l)
    
    echo "📊 Disk usage: $DISK_USAGE"
    echo "📁 Total Parquet files: $TOTAL_FILES"
    echo "📅 Files today: $TODAY_FILES"
else
    echo "⚠️  Data directory not found: data/orderbook_snapshots"
fi
echo ""

# Check logs for errors (systemd journal or log file)
echo "=== LOGS & ERRORS ==="
RECENT_ERRORS=0

# Check systemd journal if service is running via systemd
if [ "$COLLECTOR_METHOD" = "systemd" ]; then
    if command -v journalctl > /dev/null 2>&1; then
        # Check for errors in last 100 lines
        RECENT_ERRORS=$(sudo journalctl -u surveillance-collect -n 100 --no-pager 2>/dev/null | grep -iE "error|failed|disconnect|closed|warning" | wc -l)
        if [ "$RECENT_ERRORS" -gt 0 ]; then
            echo "⚠️  Recent errors in systemd journal: $RECENT_ERRORS (last 100 lines)"
            echo "   Recent errors:"
            sudo journalctl -u surveillance-collect -n 100 --no-pager 2>/dev/null | grep -iE "error|failed|disconnect|closed" | tail -3 | sed 's/^/   /'
        else
            echo "✅ No recent errors in systemd journal"
        fi
        
        # Check last activity
        LAST_LOG=$(sudo journalctl -u surveillance-collect -n 1 --no-pager -o short 2>/dev/null | tail -1)
        if [ -n "$LAST_LOG" ]; then
            echo "   Last log entry: ${LAST_LOG:0:80}..."
        fi
    fi
fi

# Check collector.log if it exists (fallback for non-systemd)
if [ -f collector.log ]; then
    LOG_ERRORS=$(tail -100 collector.log 2>/dev/null | grep -iE "error|failed|disconnect|closed" | wc -l)
    if [ "$LOG_ERRORS" -gt 0 ]; then
        if [ "$COLLECTOR_METHOD" != "systemd" ]; then
            echo "⚠️  Recent errors in collector.log: $LOG_ERRORS"
            echo "   Last errors:"
            tail -100 collector.log 2>/dev/null | grep -iE "error|failed|disconnect|closed" | tail -3 | sed 's/^/   /'
        fi
    elif [ "$COLLECTOR_METHOD" != "systemd" ]; then
        echo "✅ No recent errors in collector.log"
    fi
fi
echo ""

# Check binaries
echo "=== BINARIES ==="
BINARY_COUNT=0
if [ -f target/release/surveillance_collect ]; then
    echo "✅ Collector binary: EXISTS"
    BINARY_COUNT=$((BINARY_COUNT + 1))
else
    echo "⚠️  Collector binary: NOT FOUND (run: cargo build --release)"
fi

if [ -f target/release/surveillance_scanner ]; then
    echo "✅ Scanner binary: EXISTS"
    BINARY_COUNT=$((BINARY_COUNT + 1))
else
    echo "⚠️  Scanner binary: NOT FOUND (run: cargo build --release)"
fi

if [ -f target/release/surveillance_miner ]; then
    echo "✅ Miner binary: EXISTS"
    BINARY_COUNT=$((BINARY_COUNT + 1))
else
    echo "⚠️  Miner binary: NOT FOUND (run: cargo build --release)"
fi

if [ "$BINARY_COUNT" -eq 3 ]; then
    echo "✅ All binaries present"
fi
echo ""

# Check systemd service status if available
if command -v systemctl > /dev/null 2>&1; then
    echo "=== SYSTEMD SERVICE ==="
    if systemctl list-unit-files | grep -q surveillance-collect.service; then
        if systemctl is-enabled --quiet surveillance-collect 2>/dev/null; then
            echo "✅ Service is enabled (auto-start on boot)"
        else
            echo "⚠️  Service exists but is not enabled"
        fi
        
        STATUS=$(systemctl is-active surveillance-collect 2>/dev/null || echo "inactive")
        if [ "$STATUS" = "active" ]; then
            echo "✅ Service status: $STATUS"
        else
            echo "❌ Service status: $STATUS"
        fi
    else
        echo "ℹ️  Systemd service not found (collector may be running directly)"
    fi
    echo ""
fi

# Check universe file (for today)
echo "=== MARKET UNIVERSE ==="
TODAY=$(date -u +%Y-%m-%d)
UNIVERSE_FILE="data/metadata/venue=polymarket/date=${TODAY}/universe.jsonl"
if [ -f "$UNIVERSE_FILE" ]; then
    MARKET_COUNT=$(wc -l < "$UNIVERSE_FILE" 2>/dev/null || echo "0")
    echo "✅ Universe file exists for today ($TODAY): $MARKET_COUNT markets"
    
    # Check if it's recent (less than 24 hours old)
    FILE_AGE_HOURS=$(($(($(date +%s) - $(stat -c %Y "$UNIVERSE_FILE" 2>/dev/null || echo 0))) / 3600))
    if [ "$FILE_AGE_HOURS" -lt 24 ]; then
        echo "   Age: $FILE_AGE_HOURS hours old (fresh)"
    else
        echo "   ⚠️  Age: $FILE_AGE_HOURS hours old (consider running scanner)"
    fi
else
    echo "⚠️  Universe file not found for today: $UNIVERSE_FILE"
    echo "   Run: ./target/release/surveillance_scanner config/surveillance.toml"
fi
echo ""

# Python environment check
echo "=== PYTHON ENVIRONMENT ==="
if [ -d venv ] && [ -f venv/bin/python3 ]; then
    echo "✅ Python virtual environment exists"
    if venv/bin/python3 -c "import polars" 2>/dev/null; then
        POLARS_VERSION=$(venv/bin/python3 -c "import polars; print(polars.__version__)" 2>/dev/null)
        echo "   ✅ Polars: $POLARS_VERSION"
    else
        echo "   ⚠️  Polars not installed in venv"
    fi
else
    echo "ℹ️  Python virtual environment not found (optional for monitoring)"
    echo "   Run: ./setup_python_env.sh to create it"
fi
echo ""

# Summary
echo "=== SUMMARY ==="
if [ "$COLLECTOR_RUNNING" = true ] && [ "$RECENT_FILES" -gt 0 ]; then
    echo "✅ System is HEALTHY - Collector running and creating files"
elif [ "$COLLECTOR_RUNNING" = true ] && [ "$RECENT_FILES" -eq 0 ]; then
    echo "⚠️  Collector is RUNNING but NO FILES - Check WebSocket connection"
elif [ "$COLLECTOR_RUNNING" = false ]; then
    echo "❌ System is DOWN - Collector not running"
    if command -v systemctl > /dev/null 2>&1 && systemctl list-unit-files | grep -q surveillance-collect.service; then
        echo "   Start with: sudo systemctl start surveillance-collect"
    else
        echo "   Start with: nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &"
    fi
fi
echo ""
echo "=== End Health Check ==="
