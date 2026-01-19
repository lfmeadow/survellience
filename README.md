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
./bin/surveillance_scanner [config_path]
# or build from source: cargo run --bin surveillance_scanner [config_path]
```

Generates universe files at:
```
data/metadata/venue={venue}/date={YYYY-MM-DD}/universe.jsonl
```

### Collector

Collect order book snapshots:

```bash
./bin/surveillance_collect [config_path]
# or build from source: cargo run --bin surveillance_collect [config_path]
```

Writes Parquet files to:
```
data/orderbook_snapshots/venue={venue}/date={YYYY-MM-DD}/hour={HH}/snapshots_{timestamp}.parquet
```

### Miner

Analyze collected data:

```bash
./bin/surveillance_miner --config config/surveillance.toml mine --venue polymarket --date 2026-01-14
# or build from source: cargo run --bin surveillance_miner -- mine --venue polymarket --date 2026-01-14
```

Produces statistics at:
```
data/stats/venue={venue}/date={YYYY-MM-DD}/stats.parquet
```

### MM Viability Report

Estimate passive market-making viability:

```bash
./bin/surveillance_miner --config config/surveillance.toml mm-viability \
  --venue polymarket \
  --date 2026-01-14 \
  --hours all \
  --fee-estimate 0.0 \
  --top 20 \
  --write-report true
# or build from source: cargo run -p surveillance --bin surveillance_miner -- mm-viability ...
```

Writes report to:
```
data/reports/mm_viability/venue={venue}/date={YYYY-MM-DD}/mm_viability.parquet
```

### Rules Pipeline (Arb Detector)

The rules pipeline extracts logical constraints from market rules and detects arbitrage opportunities:

```bash
# Run full pipeline in mock mode (for testing)
cargo run --bin surveillance_rules -- run-all --mock --all-venues --date 2026-01-18

# Run individual steps
cargo run --bin surveillance_rules -- ingest --venue polymarket --date 2026-01-18
cargo run --bin surveillance_rules -- normalize --venue polymarket --date 2026-01-18
cargo run --bin surveillance_rules -- constraints --venue polymarket --date 2026-01-18
cargo run --bin surveillance_rules -- detect-arb --venue polymarket --date 2026-01-18 --mock
```

**Pipeline stages:**
1. **Ingest**: Fetch market rules text (or use mock data)
2. **Normalize**: Parse rules into canonical propositions with confidence scores
3. **Constraints**: Generate logical constraints (e.g., monotonic ladders for price barriers)
4. **Detect-Arb**: Check for constraint violations using orderbook data

**Output files:**
```
data/rules/venue={venue}/date={date}/rules.jsonl          # Raw rules text
data/logic/venue={venue}/date={date}/propositions.parquet # Normalized propositions
data/logic/venue={venue}/date={date}/constraints.parquet  # Generated constraints
data/logic/venue={venue}/date={date}/violations.parquet   # Detected violations
data/review_queue/venue={venue}/date={date}/queue.jsonl   # Low-confidence items for review
```

**Example: Inspect violations with Polars**
```python
import polars as pl
df = pl.read_parquet("data/logic/venue=polymarket/date=2026-01-18/violations.parquet")
print(df)
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
- **rules/**: Rules → Logic → Constraints → Arb Detector pipeline
  - `proposition.rs`: Core types for normalized propositions
  - `ingest.rs`: Rules text fetching and storage
  - `extract.rs`: Deterministic parsing and extraction
  - `normalize.rs`: Normalization pipeline
  - `confidence.rs`: Confidence scoring
  - `constraints.rs`: Constraint generation (monotonic ladders, etc.)
  - `arb_detector.rs`: Violation detection
  - `review_queue.rs`: Human-in-the-loop review management
  - `outputs.rs`: Parquet/JSONL output writing

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

## Repository Structure

```
├── bin/                    # Build artifacts directory (ignored by git - generate with: cargo build --release)
├── config/                 # Configuration files (surveillance.toml)
├── docs/                   # Documentation
├── scripts/                # Shell scripts and Python utilities
├── services/               # Rust source code (workspace)
├── target/                 # Build artifacts (not committed)
├── venv/                   # Python virtual environment (not committed)
├── Cargo.toml             # Rust workspace configuration
├── README.md              # This file
└── .gitignore             # Git ignore rules
```

## Quick Navigation

### For New Agents/Owners:
1. **Start Here**: `docs/QUICKSTART.md` - Complete setup guide
2. **Production Ready**: `docs/PRODUCTION_QUICKSTART.md` - Deploy to production
3. **System Status**: `docs/DEPLOYMENT_READINESS.md` - Current implementation status
4. **Run Dashboard**: `./scripts/dashboard.sh` - Live monitoring interface

### Key Scripts:
- `scripts/health_check.sh` - System health verification
- `scripts/monitor.sh [venue] [date]` - Comprehensive monitoring
- `scripts/summarize.sh [venue] [date]` - Data summary reports
- `scripts/run_mm_viability.sh [venue] [date]` - Market making analysis

### Documentation:
- `docs/MONITORING.md` - Monitoring and troubleshooting
- `docs/START_COLLECTION.md` - Data collection setup
- `docs/DESIGN.md` - System architecture and design decisions
- Venue-specific: `docs/POLYMARKET_INTEGRATION.md`, `docs/KALSHI_INTEGRATION.md`

## Production Deployment

**Ready for live data collection with Polymarket!**

See `docs/PRODUCTION_QUICKSTART.md` for deployment instructions and `docs/DEPLOYMENT_READINESS.md` for detailed status.

### Quick Start

1. Build: `cargo build --release` (or use pre-built binaries in `bin/`)
2. Configure: Edit `config/surveillance.toml` (set `[venues.polymarket].enabled = true`, `[mock].enabled = false`)
3. Discover markets: `./bin/surveillance_scanner config/surveillance.toml`
4. Collect data: `./bin/surveillance_collect config/surveillance.toml`
5. Analyze: `./bin/surveillance_miner --config config/surveillance.toml mine --venue polymarket --date $(date +%Y-%m-%d)`
6. Monitor: `./scripts/dashboard.sh` (web dashboard) or `./scripts/monitor.sh polymarket`

## TODO

- [ ] Add WebSocket reconnection with exponential backoff
- [ ] Implement Kalshi API integration (RSA-PSS authentication)
- [ ] Add Prometheus metrics export
- [ ] Add health check HTTP endpoint
- [ ] Improve error handling and recovery
- [ ] Add more comprehensive tests

## License

[Your License Here]
