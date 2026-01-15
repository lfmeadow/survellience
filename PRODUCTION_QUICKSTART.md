# Production Quick Start Guide

## Prerequisites

- Rust toolchain installed
- Disk space: ~50-100 GB recommended for data storage
- Network access to Polymarket APIs

## Initial Setup

### 1. Build Release Binaries

```bash
cargo build --release
```

Binaries will be in `target/release/`:
- `surveillance_scanner`
- `surveillance_collect`
- `surveillance_miner`

### 2. Configure

Edit `config/surveillance.toml`:

```toml
[venues.polymarket]
enabled = true
# No credentials needed for read-only access
api_key = ""
api_secret = ""
ws_url = "wss://gamma-api.polymarket.com/ws"
rest_url = "https://gamma-api.polymarket.com"

[mock]
enabled = false  # IMPORTANT: Disable mock mode for production
```

### 3. Create Data Directories (if needed)

```bash
mkdir -p data/orderbook_snapshots data/metadata data/stats
```

## Running in Production

### Step 1: Discover Markets (Run periodically, e.g., daily)

```bash
./target/release/surveillance_scanner config/surveillance.toml
```

This creates `data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl`

### Step 2: Start Data Collection (Long-running process)

```bash
# Run in foreground (for testing)
./target/release/surveillance_collect config/surveillance.toml

# Or run in background with logging
nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &

# Or use systemd (see below)
```

### Step 3: Run Analytics (Periodic, e.g., hourly or daily)

```bash
# Analyze data for a specific date
./target/release/surveillance_miner config/surveillance.toml 2024-01-15

# Or analyze today's data
./target/release/surveillance_miner config/surveillance.toml $(date +%Y-%m-%d)
```

## Monitoring

### Check Logs

```bash
# If running with nohup
tail -f collector.log

# If using systemd
journalctl -u surveillance-collect -f
```

### Monitor Data Collection

```bash
# Check for new Parquet files
find data/orderbook_snapshots -name "*.parquet" -mmin -10

# Count files created today
find data/orderbook_snapshots -name "*.parquet" -newermt "$(date +%Y-%m-%d)" | wc -l

# Check disk usage
du -sh data/orderbook_snapshots
```

### Key Things to Watch

1. **WebSocket Connection**: Look for "Connected to Polymarket WebSocket" in logs
2. **Subscription Updates**: Should see "Subscription update" messages
3. **Parquet Writes**: Should see "Flushing" messages every 5 minutes or 50k rows
4. **Errors**: Watch for WebSocket disconnection errors (may need restart)

### Health Check Script

```bash
#!/bin/bash
# health_check.sh

echo "=== Surveillance System Health Check ==="
echo ""

# Check if collector process is running
if pgrep -f surveillance_collect > /dev/null; then
    echo "✅ Collector process: RUNNING"
else
    echo "❌ Collector process: NOT RUNNING"
fi

# Check recent Parquet files
RECENT_FILES=$(find data/orderbook_snapshots -name "*.parquet" -mmin -10 | wc -l)
if [ "$RECENT_FILES" -gt 0 ]; then
    echo "✅ Recent data files: $RECENT_FILES files in last 10 minutes"
else
    echo "⚠️  No recent data files (last 10 minutes)"
fi

# Check disk space
DISK_USAGE=$(du -sh data/orderbook_snapshots 2>/dev/null | cut -f1)
echo "📊 Disk usage: $DISK_USAGE"

# Check for errors in logs (if log file exists)
if [ -f collector.log ]; then
    ERROR_COUNT=$(tail -100 collector.log | grep -i "error\|failed\|disconnect" | wc -l)
    if [ "$ERROR_COUNT" -gt 0 ]; then
        echo "⚠️  Recent errors in logs: $ERROR_COUNT"
    else
        echo "✅ No recent errors in logs"
    fi
fi

echo ""
echo "=== End Health Check ==="
```

## Systemd Service (Optional)

Create `/etc/systemd/system/surveillance-collect.service`:

```ini
[Unit]
Description=Surveillance Data Collector
After=network.target

[Service]
Type=simple
User=your-user
WorkingDirectory=/path/to/survellience
ExecStart=/path/to/survellience/target/release/surveillance_collect /path/to/survellience/config/surveillance.toml
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable surveillance-collect
sudo systemctl start surveillance-collect
sudo systemctl status surveillance-collect
```

## Troubleshooting

### WebSocket Disconnected

**Symptoms**: No new data, logs show "WebSocket error" or "WebSocket closed"

**Solution**: Restart the collector
```bash
# If using systemd
sudo systemctl restart surveillance-collect

# If using nohup
pkill -f surveillance_collect
nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &
```

### No Data Files Being Created

**Check**:
1. Is collector running? `pgrep -f surveillance_collect`
2. Are subscriptions active? Check logs for "Subscription update"
3. Is WebSocket connected? Check logs for "Connected to Polymarket WebSocket"
4. Are markets discovered? Check `data/metadata/venue=polymarket/date=*/universe.jsonl`

### High Disk Usage

**Solution**: Implement data retention policy
```bash
# Remove data older than 30 days
find data/orderbook_snapshots -type f -mtime +30 -delete
find data/metadata -type f -mtime +30 -delete
find data/stats -type f -mtime +30 -delete
```

## Expected Performance

- **Markets Discovered**: ~1000-5000 active markets
- **Subscriptions**: 200 markets (40 hot + 160 warm rotating)
- **Snapshots**: 
  - Hot markets: Every 2 seconds
  - Warm markets: Every 10 seconds
- **Parquet Files**: 
  - Created every 5 minutes or 50,000 rows
  - Size: ~50-250 MB per file
- **Data Volume**: ~14-72 GB per day per venue

## Next Steps

1. **Monitor for 24-48 hours** to validate stability
2. **Check data quality**: Verify Parquet files are readable and contain expected data
3. **Run miner**: Test analytics on collected data
4. **Plan enhancements**: 
   - Add WebSocket reconnection (see DEPLOYMENT_READINESS.md)
   - Add metrics/health checks
   - Set up data retention policies

## Support

- Check logs: `tail -f collector.log` or `journalctl -u surveillance-collect -f`
- Review `DEPLOYMENT_READINESS.md` for known limitations
- See `POLYMARKET_INTEGRATION.md` for API details
