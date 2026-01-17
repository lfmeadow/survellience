# Monitoring and Summarization Tools

## Quick Reference

### Available Tools

1. **`./scripts/monitor.sh [venue] [date]`** - Comprehensive system monitoring
2. **`./scripts/summarize.sh [venue] [date]`** - Generate and view data summaries  
3. **`./scripts/health_check.sh`** - Quick health check
4. **`./target/release/surveillance_miner [config] [venue] [date]`** - Direct analytics

## What's Available to Monitor

### 1. System Health
- **Collector Process**: Running status, PID, memory usage
- **Recent Activity**: Files created in last 10 minutes
- **Disk Usage**: Total and per-directory breakdown
- **Log Errors**: Error count and recent error messages

### 2. Data Collection Status
- **Parquet Files**: Count by hour, total files, total size
- **File Timestamps**: Most recent file creation time
- **Collection Rate**: Files per time period

### 3. Market Universe
- **Markets Discovered**: Total count from scanner
- **Markets with Token IDs**: Required for Polymarket subscriptions
- **Universe File**: Location and status

### 4. Statistics Cache
- **Stats File**: Exists and row count
- **Summary Statistics**: 
  - Average depth across all markets
  - Average spread
  - Total updates
  - Average updates per market

### 5. Analytics Output (from Miner)

The miner computes and outputs:

#### Metrics Per Market/Outcome:
- **avg_depth**: Average order book depth (sum of best bid/ask sizes)
- **avg_spread**: Average bid-ask spread
- **update_count**: Number of snapshots collected

#### Rankings:
- **Top 10 by Average Depth**: Markets with most liquidity
- **Top 10 by Tightest Spread**: Most efficient markets
- **Top 10 Most Active**: Markets with most updates

### 6. Raw Data Files

**Location**: `data/orderbook_snapshots/venue={venue}/date={YYYY-MM-DD}/hour={HH}/`

**Schema** (Parquet):
- `ts_recv`: Timestamp (epoch ms UTC)
- `venue`: Venue name
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

## Usage Examples

### Daily Monitoring
```bash
# Check system health
./scripts/monitor.sh polymarket

# Generate statistics for today
./scripts/summarize.sh polymarket
```

### Historical Analysis
```bash
# Analyze specific date
./target/release/surveillance_miner config/surveillance.toml polymarket 2026-01-13

# Compare multiple dates
for date in 2026-01-13 2026-01-14 2026-01-15; do
    ./target/release/surveillance_miner config/surveillance.toml polymarket $date
done
```

### Quick Health Check
```bash
./scripts/health_check.sh
```

## Data Analysis Options

### Using Polars (Python)
```python
import polars as pl

# Read stats
stats = pl.read_parquet("data/stats/venue=polymarket/date=2026-01-14/stats.parquet")
print(stats.describe())

# Read snapshots
snapshots = pl.scan_parquet("data/orderbook_snapshots/venue=polymarket/date=2026-01-14/**/*.parquet").collect()

# Custom analysis
top_markets = snapshots.group_by("market_id").agg([
    pl.count().alias("snapshots"),
    pl.mean("spread").alias("avg_spread")
]).sort("snapshots", descending=True)
```

### Using Command Line
```bash
# Count files
find data/orderbook_snapshots -name "*.parquet" | wc -l

# Check file sizes
du -sh data/orderbook_snapshots/venue=polymarket/date=*/

# View stats (if CSV exists)
cat data/stats/venue=polymarket/date=*/stats.csv | column -t -s','
```

## Key Metrics to Watch

### System Metrics
- **Collector Running**: Should always be true during collection
- **Recent Files**: Should see new files every 5 minutes (flush interval)
- **Disk Growth**: ~14-72 GB/day per venue (monitor disk space)

### Data Quality Metrics
- **Update Count**: Should be > 0 for active markets
- **Spread Values**: Should be reasonable (not NaN, not extreme)
- **File Count**: Should increase over time

### Market Coverage
- **Markets Discovered**: Check universe.jsonl
- **Markets with Token IDs**: Required for Polymarket
- **Active Subscriptions**: Should match scheduler config

## Output Files Summary

| File Type | Location | Purpose |
|-----------|----------|---------|
| **Parquet Snapshots** | `data/orderbook_snapshots/venue={venue}/date={date}/hour={HH}/` | Raw order book data |
| **Stats Cache** | `data/stats/venue={venue}/date={date}/stats.parquet` | Aggregated statistics |
| **Universe** | `data/metadata/venue={venue}/date={date}/universe.jsonl` | Market discovery results |

## Next Steps

For more advanced monitoring:
- Set up automated daily summaries (cron job)
- Create dashboards (Grafana/Prometheus)
- Implement alerting on anomalies
- Add data quality checks
- Build web UI for browsing

See `MONITORING.md` for detailed documentation.
