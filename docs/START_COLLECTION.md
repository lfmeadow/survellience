# How to Start Data Collection

## Quick Start (3 Steps)

### Step 1: Discover Markets
Run the scanner to discover available markets from Polymarket:

```bash
./target/release/surveillance_scanner config/surveillance.toml
```

**What it does:**
- Connects to Polymarket REST API
- Fetches all available markets
- Extracts token IDs (clobTokenIds) for WebSocket subscriptions
- Writes universe file: `data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl`

**Expected output:**
```
INFO surveillance::scanner::scanner: Scanning venue: polymarket
INFO surveillance::venue::polymarket: Fetching markets from Polymarket: https://gamma-api.polymarket.com/markets
INFO surveillance::venue::polymarket: Discovered N markets from Polymarket
INFO surveillance::scanner::scanner: Wrote universe file: "data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl" (N markets)
```

### Step 2: Start Data Collection
Run the collector to start collecting order book snapshots:

```bash
./target/release/surveillance_collect config/surveillance.toml
```

**What it does:**
- Connects to Polymarket WebSocket
- Loads market universe from scanner output
- Subscribes to markets (rotating hot/warm sets)
- Receives order book updates
- Writes snapshots to Parquet files every 5 minutes or 50k rows

**Expected output:**
```
INFO Starting collector for venue: polymarket
INFO Connecting to Polymarket WebSocket: wss://ws-subscriptions-clob.polymarket.com/ws/market
INFO Connected to Polymarket WebSocket
INFO Rotating subscriptions for polymarket
INFO Subscribing to N token IDs: [...]
INFO Subscription message sent for N token IDs
```

**To run in background:**
```bash
nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &
```

### Step 3: Monitor Collection (Optional)
Check that data is being collected:

```bash
# Quick health check
./scripts/health_check.sh

# Detailed monitoring
./scripts/monitor.sh polymarket

# Watch logs (if running in foreground)
# Or if running in background:
tail -f collector.log
```

## Configuration

Before starting, ensure `config/surveillance.toml` is configured:

```toml
[venues.polymarket]
enabled = true  # Must be true
api_key = ""    # Not needed for read-only
api_secret = "" # Not needed for read-only

[mock]
enabled = false  # Must be false for real data
```

## What Happens During Collection

1. **WebSocket Connection**: Connects to Polymarket CLOB WebSocket
2. **Market Selection**: Scheduler selects top markets based on scoring
3. **Subscription**: Subscribes to selected markets using token IDs
4. **Order Book Updates**: Receives real-time order book updates
5. **Snapshots**: Creates snapshots every 2s (hot) or 10s (warm)
6. **File Writing**: Writes Parquet files every 5 minutes or 50k rows

## Data Output Locations

- **Snapshots**: `data/orderbook_snapshots/venue=polymarket/date=YYYY-MM-DD/hour=HH/snapshots_*.parquet`
- **Universe**: `data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl`
- **Stats**: `data/stats/venue=polymarket/date=YYYY-MM-DD/stats.parquet` (after running miner)

## Verification

After a few minutes, verify data is being collected:

```bash
# Check for new files
find data/orderbook_snapshots -name "*.parquet" -mmin -10

# Check file count
find data/orderbook_snapshots -name "*.parquet" | wc -l

# Check disk usage
du -sh data/orderbook_snapshots
```

## Troubleshooting

### No files being created?
1. Check collector is running: `pgrep -f surveillance_collect`
2. Check WebSocket connection in logs
3. Verify markets were discovered: `cat data/metadata/venue=polymarket/date=*/universe.jsonl | wc -l`
4. Check subscriptions are active in logs

### WebSocket disconnection?
- Collector will continue but won't receive updates
- Restart collector: `pkill -f surveillance_collect && ./target/release/surveillance_collect config/surveillance.toml`

### No markets discovered?
- Check network connectivity
- Verify Polymarket API is accessible
- Check logs for API errors

## Next Steps

After collecting data:

1. **Generate Statistics**:
   ```bash
   ./target/release/surveillance_miner config/surveillance.toml polymarket
   ```

2. **View Summary**:
   ```bash
   ./scripts/summarize.sh polymarket
   ```

3. **Monitor System**:
   ```bash
   ./scripts/monitor.sh polymarket
   ```

## Running as a Service

For production, consider using systemd (see `PRODUCTION_QUICKSTART.md` for systemd service example).
