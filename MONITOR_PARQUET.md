# Monitoring Parquet Files

This guide explains how to monitor the Parquet files created by the surveillance collector.

## Quick Commands

### Check File Count and Size
```bash
# Count all parquet files
find data/orderbook_snapshots -name "*.parquet" -type f | wc -l

# Show total size
du -sh data/orderbook_snapshots

# List files by venue/date/hour
find data/orderbook_snapshots -name "*.parquet" -type f | sort
```

### Monitor Recent Activity
```bash
# Files created in last 10 minutes
find data/orderbook_snapshots -name "*.parquet" -type f -mmin -10

# Latest file
find data/orderbook_snapshots -name "*.parquet" -type f -printf '%T@ %p\n' | sort -n | tail -1

# Files by hour for today
ls -lh data/orderbook_snapshots/venue=polymarket/date=$(date +%Y-%m-%d)/hour=*/
```

## Using the Monitor Script

### Basic Usage
```bash
# Monitor today's files
./monitor.sh polymarket

# Monitor specific date
./monitor.sh polymarket 2026-01-14
```

The monitor script shows:
- System health (collector process status)
- Parquet files by hour
- Total file count and size
- Recent activity (last 10 minutes)
- Market universe status
- Statistics cache status
- Disk usage

### Parquet-Specific Monitor
```bash
# Quick parquet file status
./monitor_parquet.sh polymarket

# Monitor specific date
./monitor_parquet.sh polymarket 2026-01-14

# Watch mode (updates every 5 seconds)
./monitor_parquet.sh polymarket --watch
```

## Using Command Line Tools

### File Statistics
```bash
# Count files by hour directory
for dir in data/orderbook_snapshots/venue=polymarket/date=$(date +%Y-%m-%d)/hour=*; do
    echo "$(basename $dir): $(find "$dir" -name "*.parquet" -type f | wc -l) files"
done

# File sizes
du -h data/orderbook_snapshots/venue=polymarket/date=$(date +%Y-%m-%d)/hour=*/*.parquet

# Latest 10 files
find data/orderbook_snapshots -name "*.parquet" -type f -printf '%T@ %p\n' | sort -n | tail -10 | cut -d' ' -f2-
```

### Using Python/Polars (if available)
```bash
# Quick stats
python3 << EOF
import polars as pl
from pathlib import Path

snapshot_dir = Path("data/orderbook_snapshots/venue=polymarket/date=$(date +%Y-%m-%d)")
parquet_files = list(snapshot_dir.rglob("*.parquet"))

print(f"Total files: {len(parquet_files)}")

if parquet_files:
    # Read first file to check schema
    df = pl.read_parquet(parquet_files[0])
    print(f"\nSchema: {df.columns}")
    print(f"Rows in first file: {len(df)}")
    print(f"Sample data:\n{df.head(3)}")
EOF
```

## Data Structure

Parquet files are organized as:
```
data/orderbook_snapshots/
  venue=polymarket/
    date=2026-01-14/
      hour=20/
        snapshots_2026-01-14T20-50.parquet
        snapshots_2026-01-14T20-55.parquet
      hour=21/
        snapshots_2026-01-14T21-00.parquet
```

### File Naming Convention
- Format: `snapshots_{YYYY-MM-DDTHH-MM}.parquet`
- Bucket: 5-minute intervals (default)
- Example: `snapshots_2026-01-14T20-50.parquet` contains data from 20:50-20:54

### Schema
Each Parquet file contains rows with:
- `ts_recv`: Timestamp (epoch ms UTC)
- `venue`: Venue name (e.g., "polymarket")
- `market_id`: Market identifier
- `outcome_id`: Outcome identifier
- `seq`: Sequence number
- `best_bid_px`, `best_bid_sz`: Best bid price/size
- `best_ask_px`, `best_ask_sz`: Best ask price/size
- `mid`: Mid price
- `spread`: Spread
- `bid_px`, `bid_sz`: List of bid prices/sizes (top K levels)
- `ask_px`, `ask_sz`: List of ask prices/sizes (top K levels)
- `status`: Status string
- `source_ts`: Source timestamp (if available)

## Monitoring Checklist

1. **File Creation Rate**
   - Check files created in last 5-10 minutes
   - Should see new files every 5 minutes (bucket interval)
   - HOT markets: snapshots every 2 seconds
   - WARM markets: snapshots every 10 seconds

2. **File Sizes**
   - Typical file: 10-100 KB (depends on number of markets)
   - Files should grow over time as more snapshots are written
   - Very small files (< 1 KB) may indicate issues

3. **Directory Structure**
   - Check that hour directories are created correctly
   - Files should be in `venue={venue}/date={date}/hour={HH}/`
   - Files should be named correctly

4. **Data Quality**
   - Use miner to analyze data quality
   - Check for missing markets/outcomes
   - Verify timestamps are reasonable

## Troubleshooting

### No Files Being Created
- Check collector is running: `pgrep -f surveillance_collect`
- Check collector logs: `tail -f collector.log`
- Verify WebSocket connection is active
- Check subscriptions are active

### Files Too Small
- May indicate low subscription count
- Check scheduler is selecting markets
- Verify markets have order book data

### Files Too Large
- May indicate too many subscriptions
- Check subscription limits in config
- Consider reducing `max_subs` or `hot_count`

### Files Not Flushing
- Files are flushed every 5 seconds (default)
- Check parquet writer logs
- Verify disk space is available
- Check write permissions

## Continuous Monitoring

For continuous monitoring, use watch mode:
```bash
# Watch parquet files
watch -n 5 './monitor_parquet.sh polymarket'

# Or use the script's built-in watch mode
./monitor_parquet.sh polymarket --watch
```

For automated monitoring, you can:
- Set up cron job to check file counts
- Monitor disk usage
- Alert on missing files
- Track file growth rates
