# Project Status

**Last Updated:** 2026-01-18

## Overview

Market Surveillance System for prediction markets (Polymarket, Kalshi). Collects order book data, analyzes market making viability, and provides dashboards for monitoring.

## Current Configuration

### Market Filtering (Active)

Located in `config/surveillance.toml`:

```toml
exclude_title_patterns = ["Up or Down"]  # Excludes short-term crypto prediction markets
min_hours_until_close = 24.0             # Excludes all game-day betting
```

**Rationale:** Short-term markets (crypto "Up or Down" 5-15 min windows, same-day sports betting) have poor MM viability due to:
- Extreme adverse selection (real-time observable underlying prices)
- Insufficient time for inventory management
- Binary resolution risk

See `docs/MM_VIABILITY_ANALYSIS.md` for detailed analysis.

### What's Being Collected

After filtering, the system collects data on ~20,000 markets including:
- Political/election markets (weeks/months out)
- Championship futures (Super Bowl, NBA Finals, etc.)
- Season awards and player props (long-dated)
- Economic prediction markets

These have better MM viability characteristics due to gradual information arrival and genuine uncertainty.

## Architecture

```
Scanner (surveillance_scanner)
    ↓ Fetches markets from Polymarket API, applies filters
    ↓ Writes universe.jsonl
    
Collector (surveillance_collect)
    ↓ Reads universe, subscribes to WebSocket
    ↓ Writes parquet snapshots
    
Miner (surveillance_miner)
    ↓ Reads parquet files
    ↓ Computes MM viability metrics
    ↓ Writes reports
```

## Key Files

| File | Purpose |
|------|---------|
| `config/surveillance.toml` | Main configuration (filters, venues, storage) |
| `services/surveillance/src/venue/polymarket.rs` | Polymarket API integration and filtering logic |
| `services/surveillance/src/config.rs` | Config struct with filter fields |
| `scripts/dashboard.py` | Terminal dashboard |
| `scripts/dashboard_web.py` | Web dashboard (HTML) |
| `docs/MM_VIABILITY_ANALYSIS.md` | Analysis of MM viability on different market types |
| `docs/POLYMARKET_INTEGRATION.md` | Polymarket market structure documentation |

## Data Layout

```
data/
├── metadata/venue=polymarket/date=YYYY-MM-DD/
│   └── universe.jsonl          # Market metadata (titles, IDs, close times)
├── orderbook_snapshots/venue=polymarket/date=YYYY-MM-DD/hour=HH/
│   └── snapshots_*.parquet     # Order book snapshots
├── reports/mm_viability/venue=polymarket/date=YYYY-MM-DD/
│   └── mm_viability.parquet    # MM viability analysis results
└── trades/venue=polymarket/date=YYYY-MM-DD/hour=HH/
    └── trades_*.parquet        # Trade execution data
```

## Common Commands

```bash
# Scan for markets (applies filters, ~40s)
./scripts/run_scanner.sh

# Start data collection
./scripts/start_collector.sh

# Stop data collection
./scripts/stop_collector.sh

# View terminal dashboard
./scripts/dashboard.sh

# Show market data summary
./scripts/show_market_data_points.sh polymarket YYYY-MM-DD

# Run MM viability analysis
./scripts/run_mm_viability.sh polymarket YYYY-MM-DD

# Show MM viability report
python3 scripts/show_mm_report.py
```

## Recent Changes (2026-01-18)

1. **Added market filtering** - `exclude_title_patterns` and `min_hours_until_close` config options
2. **Increased duration filter** - Changed from 1 hour to 24 hours to exclude game-day betting
3. **Fixed dashboard N/A issue** - Dashboards now load market titles from all available universe files
4. **Added MM viability analysis doc** - Documents why short-term markets are unsuitable for MM
5. **Added Polymarket market structure doc** - Explains time-boxed markets and independent market IDs

## Known Issues / TODOs

- Scanner takes ~40s due to API pagination (25k+ markets)
- Consider adding scanner caching/incremental updates
- MM viability analysis requires sufficient data (default min_rows=1000)

## Key Insights

### Polymarket Market Types

1. **Short-term crypto ("Up or Down")** - 5-15 min windows, filtered out, poor MM viability
2. **Game-day sports betting** - Same-day resolution, filtered out, moderate MM viability
3. **Futures/championships** - Days/weeks out, collected, good MM viability candidates

### MM Viability Factors

| Factor | Good for MM | Bad for MM |
|--------|-------------|------------|
| Duration | Days/weeks | Minutes/hours |
| Observable underlying | No real-time price | Binance/live score |
| Information arrival | Gradual | Instant |
| Adverse selection | Low | High (toxicity > -0.5) |

## Contact

Repository: https://github.com/lfmeadow/survellience
