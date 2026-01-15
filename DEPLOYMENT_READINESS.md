# Deployment Readiness Checklist

## ✅ Ready for Live Deployment

### Core Functionality
- ✅ Polymarket REST API integration (market discovery)
- ✅ Polymarket WebSocket integration (order book updates)
- ✅ No credentials required for Polymarket read-only access
- ✅ Parquet file writing with atomic writes
- ✅ Hive-style partitioning (`venue=X/date=Y/hour=H`)
- ✅ Market rotation and subscription management
- ✅ Snapshot generation and storage
- ✅ Data mining with Polars

### Configuration
- ✅ Config file supports Polymarket
- ✅ Credentials optional for Polymarket (public endpoints)
- ✅ All required settings configurable

### Data Pipeline
- ✅ Scanner → Collector → Parquet Writer → Miner pipeline functional
- ✅ End-to-end tested in mock mode
- ✅ Parquet files readable by Polars

## ⚠️ Known Limitations / Enhancements Needed

### Reconnection Logic
- ⚠️ **WebSocket reconnection**: Currently connects once at startup. If connection drops, collector will continue but won't receive updates.
  - **Impact**: Medium - Data collection will pause if WebSocket disconnects
  - **Workaround**: Restart collector if disconnection detected
  - **Recommendation**: Add automatic reconnection with exponential backoff

### Error Handling
- ✅ Basic error handling in place
- ⚠️ No exponential backoff for API retries
- ⚠️ No circuit breaker pattern

### Monitoring
- ✅ Logging via `tracing`
- ⚠️ No metrics export (Prometheus/StatsD)
- ⚠️ No health check endpoints

### Rate Limiting
- ✅ Subscription churn limiting implemented
- ⚠️ No explicit API rate limiting (relies on venue limits)

## 🚀 Deployment Steps

### 1. Pre-Deployment
```bash
# Build release binaries
cargo build --release

# Test configuration
cargo run --release --bin surveillance_scanner config/surveillance.toml
```

### 2. Configuration
Update `config/surveillance.toml`:
```toml
[venues.polymarket]
enabled = true
api_key = ""  # Not needed for read-only
api_secret = ""  # Not needed for read-only
ws_url = "wss://gamma-api.polymarket.com/ws"
rest_url = "https://gamma-api.polymarket.com"

[mock]
enabled = false  # Disable mock mode
```

### 3. Run Scanner (One-time or Periodic)
```bash
# Discover markets
./target/release/surveillance_scanner config/surveillance.toml
```

### 4. Run Collector (Long-running)
```bash
# Start data collection
./target/release/surveillance_collect config/surveillance.toml
```

### 5. Run Miner (Periodic, e.g., hourly/daily)
```bash
# Analyze collected data
./target/release/surveillance_miner config/surveillance.toml 2024-01-15
```

### 6. Monitoring
- Monitor logs for connection status
- Check `data/orderbook_snapshots/` for new Parquet files
- Monitor disk space (Parquet files can grow quickly)
- Watch for WebSocket disconnection errors

## 📊 Expected Behavior

### Normal Operation
- Scanner: Discovers markets, writes `data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl`
- Collector: 
  - Connects to Polymarket WebSocket
  - Subscribes to markets (rotating hot/warm sets)
  - Generates snapshots every 2s (hot) / 10s (warm)
  - Writes Parquet files every 5 minutes or 50k rows
- Miner: Reads Parquet files, computes metrics, writes stats cache

### Data Volume Estimates
- **Snapshots**: ~1-5 KB per snapshot
- **50k snapshots**: ~50-250 MB per Parquet file
- **Per hour**: ~600-3000 MB (depending on subscription count)
- **Per day**: ~14-72 GB per venue

## 🔧 Production Recommendations

### Before Full Production
1. **Add WebSocket Reconnection**: Implement automatic reconnection with backoff
2. **Add Health Checks**: Monitor collector status
3. **Add Metrics**: Export metrics for monitoring
4. **Add Alerting**: Alert on disconnections, disk space, errors

### Optional Enhancements
- [ ] Add Prometheus metrics export
- [ ] Add health check HTTP endpoint
- [ ] Add graceful shutdown handling
- [ ] Add data retention policies
- [ ] Add compression for Parquet files
- [ ] Add S3/cloud storage backend

## ✅ Ready to Deploy?

**Yes, with caveats:**
- ✅ Core functionality is complete and tested
- ✅ Polymarket integration is functional
- ⚠️ Monitor for WebSocket disconnections (may need manual restart)
- ⚠️ Monitor disk space usage
- ⚠️ Consider adding reconnection logic for production reliability

**Recommended**: Start with a limited deployment (fewer markets, shorter duration) to validate behavior before full-scale production.
