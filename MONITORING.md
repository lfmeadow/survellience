# Monitoring and Data Summarization Guide

## Available Tools

### 1. Health Check Script (`health_check.sh`)

Quick system health check:
```bash
./health_check.sh
```

**Checks:**
- Collector process status
- Recent data files (last 10 minutes)
- Disk usage
- Log errors
- Binary availability

### 2. Monitor Script (`monitor.sh`)

Comprehensive monitoring and status:
```bash
# Monitor today's data
./monitor.sh polymarket

# Monitor specific date
./monitor.sh polymarket 2026-01-14
```

**Shows:**
- System health (process, memory)
- Data collection status (files by hour, totals)
- Market universe status
- Statistics cache status
- Disk usage breakdown
- Recent log activity
- Recommendations

### 3. Summarize Script (`summarize.sh`)

Generate and view data summaries:
```bash
# Summarize today's data
./summarize.sh polymarket

# Summarize specific date
./summarize.sh polymarket 2026-01-14
```

**Actions:**
- Runs the miner to generate statistics
- Displays statistics cache contents
- Lists data files

### 4. Miner Binary (`surveillance_miner`)

Direct analytics tool:
```bash
# Analyze today's data
./target/release/surveillance_miner config/surveillance.toml polymarket

# Analyze specific date
./target/release/surveillance_miner config/surveillance.toml polymarket 2026-01-14
```

**Outputs:**
- Total markets/outcomes analyzed
- Top 10 markets by average depth
- Top 10 markets by tightest spread
- Top 10 most active markets (by update count)
- Writes stats cache to `data/stats/venue={venue}/date={date}/stats.csv`

## Data Locations

### Order Book Snapshots
```
data/orderbook_snapshots/venue={venue}/date={YYYY-MM-DD}/hour={HH}/snapshots_{timestamp}.parquet
```

**Schema:**
- `ts_recv`: Timestamp (epoch ms)
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

### Statistics Cache
```
data/stats/venue={venue}/date={YYYY-MM-DD}/stats.csv
```

**Schema:**
- `market_id`: Market identifier
- `outcome_id`: Outcome identifier
- `avg_depth`: Average order book depth (sum of best bid/ask sizes)
- `avg_spread`: Average spread
- `update_count`: Number of snapshots collected

### Market Universe
```
data/metadata/venue={venue}/date={YYYY-MM-DD}/universe.jsonl
```

**Schema:**
- `market_id`: Market identifier (condition ID for Polymarket)
- `title`: Market title/question
- `outcome_ids`: List of outcome identifiers
- `close_ts`: Market close timestamp (epoch ms)
- `status`: Market status ("active", "closed", "inactive")
- `tags`: List of tags
- `token_ids`: List of token IDs (clobTokenIds for Polymarket CLOB WebSocket)

## Metrics Computed

### By the Miner

1. **Average Depth**: `mean(best_bid_sz + best_ask_sz)`
   - Measures liquidity at the top of the book
   - Higher = more liquidity

2. **Average Spread**: `mean(spread)`
   - Measures bid-ask spread
   - Lower = tighter spread, more efficient market

3. **Update Count**: `count(ts_recv)`
   - Number of snapshots collected
   - Higher = more active market

### Available in Raw Data

- **Best Bid/Ask**: Top of book prices and sizes
- **Full Depth**: Top K levels (configurable, default 50)
- **Mid Price**: `(best_bid_px + best_ask_px) / 2`
- **Spread**: `best_ask_px - best_bid_px`
- **Sequence Numbers**: Monotonic sequence per market/outcome

## Monitoring Workflow

### Daily Operations

1. **Morning Check**:
   ```bash
   ./monitor.sh polymarket
   ```

2. **Generate Statistics** (after data collection):
   ```bash
   ./summarize.sh polymarket
   ```

3. **Check Health** (periodic):
   ```bash
   ./health_check.sh
   ```

### Weekly/Monthly Analysis

1. **Analyze Multiple Dates**:
   ```bash
   for date in 2026-01-13 2026-01-14 2026-01-15; do
       ./target/release/surveillance_miner config/surveillance.toml polymarket $date
   done
   ```

2. **Compare Statistics**:
   ```bash
   # View stats for different dates
   cat data/stats/venue=polymarket/date=*/stats.csv
   ```

## Data Analysis Examples

### Using Polars (Python)

```python
import polars as pl

# Read all snapshots for a date
df = pl.scan_parquet("data/orderbook_snapshots/venue=polymarket/date=2026-01-14/**/*.parquet").collect()

# Basic stats
print(df.describe())

# Top markets by volume
top_markets = df.group_by("market_id").agg([
    pl.count().alias("snapshots"),
    pl.mean("spread").alias("avg_spread"),
    (pl.mean("best_bid_sz") + pl.mean("best_ask_sz")).alias("avg_depth")
]).sort("snapshots", descending=True).head(10)

print(top_markets)
```

### Using Command Line Tools

```bash
# Count snapshots per market
find data/orderbook_snapshots/venue=polymarket/date=2026-01-14 -name "*.parquet" -exec basename {} \; | wc -l

# Check file sizes
du -sh data/orderbook_snapshots/venue=polymarket/date=*/

# Find most recent files
find data/orderbook_snapshots -name "*.parquet" -type f -mtime -1 | head -10
```

## Key Metrics to Watch

### System Health
- **Collector Running**: Process should be active
- **Recent Files**: Should see new files every 5 minutes (or per flush interval)
- **Disk Usage**: Monitor growth rate (~14-72 GB/day per venue)

### Data Quality
- **Update Count**: Should be > 0 for active markets
- **Spread Values**: Should be reasonable (not NaN or extreme)
- **Sequence Numbers**: Should be monotonic (check for gaps)

### Market Coverage
- **Markets Discovered**: Check universe.jsonl for market count
- **Markets with Token IDs**: Required for Polymarket subscriptions
- **Active Subscriptions**: Should match scheduler configuration

## Troubleshooting

### No Recent Files
1. Check collector is running: `pgrep -f surveillance_collect`
2. Check WebSocket connection in logs
3. Verify subscriptions are active
4. Check disk space

### Empty Statistics
1. Verify Parquet files exist
2. Check file permissions
3. Run miner manually to see errors
4. Verify Polars can read the files

### High Disk Usage
1. Implement data retention policy
2. Compress old Parquet files
3. Archive to cold storage
4. Adjust flush intervals

## Future Enhancements

- [ ] Real-time dashboard (Prometheus/Grafana)
- [ ] Automated daily summaries
- [ ] Alerting on anomalies
- [ ] Data quality checks
- [ ] Performance metrics export
- [ ] Web UI for browsing data
