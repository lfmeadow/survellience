# Market Surveillance System

A Rust-based market surveillance and universe discovery system for prediction markets (Polymarket and Kalshi).

## Overview

This system collects order book snapshots across many markets/tickers using rotating subscriptions, stores them to Parquet files in Hive-style partitions, and produces basic "interestingness" metrics to help choose markets and build canonical mappings.

## Features

- **Market Discovery**: Scanner discovers and catalogs available markets
- **Data Collection**: Collector subscribes to markets and captures order book snapshots
- **Storage**: Parquet files with Hive-style partitioning (venue/date/hour)
- **Analytics**: Miner analyzes collected data and produces market statistics
- **Mock Mode**: Full system works in mock mode for testing without API credentials

## Building

```bash
cargo build --release
```

## Configuration

Edit `config/surveillance.toml` to configure:

- Data directory
- Venue settings (Polymarket, Kalshi)
- Subscription limits and rotation periods
- Storage settings (top K levels, flush intervals)
- Mock mode settings

## Usage

### Scanner

Discover and catalog markets:

```bash
cargo run --bin surveillance_scanner [config_path]
```

Generates universe files at:
```
data/metadata/venue={venue}/date={YYYY-MM-DD}/universe.jsonl
```

### Collector

Collect order book snapshots:

```bash
cargo run --bin surveillance_collect [config_path]
```

Writes Parquet files to:
```
data/orderbook_snapshots/venue={venue}/date={YYYY-MM-DD}/hour={HH}/snapshots_{timestamp}.parquet
```

### Miner

Analyze collected data:

```bash
cargo run --bin surveillance_miner -- mine --venue polymarket --date 2026-01-14
```

Produces statistics at:
```
data/stats/venue={venue}/date={YYYY-MM-DD}/stats.parquet
```

### MM Viability Report

Estimate passive market-making viability:

```bash
cargo run -p surveillance --bin surveillance_miner -- mm-viability \
  --venue polymarket \
  --date 2026-01-14 \
  --hours all \
  --fee-estimate 0.0 \
  --top 20 \
  --write-report true
```

Writes report to:
```
data/reports/mm_viability/venue={venue}/date={YYYY-MM-DD}/mm_viability.parquet
```

## Mock Mode

Set `[mock] enabled = true` in the config to run without API credentials. The system will generate synthetic market data for testing.

## Architecture

- **scanner/**: Market universe discovery
- **collector/**: WebSocket subscription management and snapshot collection
- **scheduler/**: Subscription rotation and market selection
- **storage/**: Parquet file writing with batching
- **analytics/**: Data analysis using Polars
- **venue/**: Venue adapters (Polymarket, Kalshi, Mock)

## Testing

Run tests:

```bash
cargo test
```

## Status

- ✅ Core infrastructure (config, schema, timebucket)
- ✅ Venue abstraction with mock implementation
- ✅ Scanner module
- ✅ Scheduler module
- ✅ Collector module
- ✅ Storage/Parquet writer
- ✅ Analytics/Miner
- ✅ Polymarket integration (REST API + WebSocket) - **Ready for production**
- 🚧 Kalshi integration (structure ready, needs API implementation)

## Production Deployment

**Ready for live data collection with Polymarket!**

See `PRODUCTION_QUICKSTART.md` for deployment instructions and `DEPLOYMENT_READINESS.md` for detailed status.

### Quick Start

1. Build: `cargo build --release`
2. Configure: Edit `config/surveillance.toml` (set `[venues.polymarket].enabled = true`, `[mock].enabled = false`)
3. Discover markets: `./target/release/surveillance_scanner config/surveillance.toml`
4. Collect data: `./target/release/surveillance_collect config/surveillance.toml`
5. Analyze: `./target/release/surveillance_miner config/surveillance.toml $(date +%Y-%m-%d)`

## TODO

- [ ] Add WebSocket reconnection with exponential backoff
- [ ] Implement Kalshi API integration (RSA-PSS authentication)
- [ ] Add Prometheus metrics export
- [ ] Add health check HTTP endpoint
- [ ] Improve error handling and recovery
- [ ] Add more comprehensive tests

## License

[Your License Here]
