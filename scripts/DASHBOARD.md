# Live Dashboard Guide

## Overview

The live dashboard (`dashboard.py`) provides an interactive, real-time view of the market surveillance system with the ability to explore markets in detail. It displays system health, data collection status, market universe, and allows drilling down into specific market data with full descriptions.

## Features

- **Live System Status**: Real-time collector status, data collection metrics
- **Market Exploration**: Browse all discovered markets with their full titles/descriptions
- **Market Drill-Down**: View detailed order book data for specific markets and outcomes
- **Auto-Refresh**: Automatically updates every 5 seconds (configurable)
- **Interactive Navigation**: Keyboard-driven interface for easy exploration

## Prerequisites

The dashboard requires Python 3 with the `polars` library:

```bash
# Option 1: Use virtual environment (recommended)
./setup_python_env.sh
source venv/bin/activate

# Option 2: Install globally
pip install polars
```

## Usage

### Quick Start

```bash
# Run dashboard for default venue (polymarket) and today's date
./dashboard.sh

# Or specify venue and date
./dashboard.sh polymarket 2026-01-14

# Or use Python directly
python3 dashboard.py polymarket --date 2026-01-14
```

### Command Line Options

```bash
dashboard.py [VENUE] [OPTIONS]

Arguments:
  VENUE              Venue name (default: polymarket)

Options:
  --date DATE        Date in YYYY-MM-DD format (default: today UTC)
  --refresh SECONDS  Refresh interval in seconds (default: 5)
```

### Examples

```bash
# View today's data for polymarket
./dashboard.sh polymarket

# View specific date
./dashboard.sh polymarket 2026-01-14

# Custom refresh interval (update every 2 seconds)
python3 dashboard.py polymarket --refresh 2

# View different date
python3 dashboard.py polymarket --date 2026-01-13
```

## Navigation

### Overview Screen (Default)

Shows:
- System health (collector status, memory usage)
- Data collection metrics (file counts, sizes, recent activity)
- Market universe statistics
- Top 10 markets by update count with full titles

**Controls:**
- `m` - Navigate to markets list
- `r` - Refresh immediately
- `q` - Quit dashboard

### Markets List Screen

Shows all discovered markets with:
- Full market titles/descriptions
- Market status (active/inactive)
- Outcome IDs
- Statistics (update counts per outcome)

**Controls:**
- `↑` / `↓` or `j` / `k` - Navigate through markets
- `Enter` - View detailed market data
- `b` - Back to overview
- `q` - Quit dashboard

### Market Detail Screen

Shows detailed information for a selected market:
- Full market title and metadata
- Market ID, status, tags, close date
- For each outcome:
  - Number of snapshots collected
  - Latest snapshot data (best bid/ask, mid price, spread)
  - Statistics (average spread, min/max spread, average depth)

**Controls:**
- `b` - Back to markets list
- `q` - Quit dashboard

## What's Displayed

### Overview Screen

1. **System Health**
   - Collector process status (running/not running)
   - Process ID and memory usage (if running)

2. **Data Collection**
   - Total Parquet files collected
   - Total data size (GB)
   - Recent files (last 10 minutes)
   - Hours with data

3. **Market Universe**
   - Total markets discovered
   - Markets with token IDs (for subscriptions)

4. **Top 10 Markets**
   - Market titles (not just IDs!)
   - Outcome IDs
   - Update counts
   - Average spread
   - Average depth

### Markets List Screen

- Full market titles (up to 65 characters)
- Market status indicators (🟢 active, 🔴 inactive)
- Market IDs (truncated)
- Outcome IDs
- Statistics per outcome (update counts)

### Market Detail Screen

- Complete market information:
  - Full title
  - Market ID
  - Status
  - All outcome IDs
  - Tags
  - Close date/timestamp
- For each outcome:
  - Number of snapshots collected
  - Latest snapshot:
    - Timestamp
    - Best bid price and size
    - Best ask price and size
    - Mid price
    - Spread
  - Aggregated statistics:
    - Average spread
    - Min/max spread
    - Average depth

## Tips

1. **First Time Use**: Run the scanner first to discover markets:
   ```bash
   ./target/release/surveillance_scanner config/surveillance.toml
   ```

2. **Generate Statistics**: For best results, run the miner to generate statistics cache:
   ```bash
   ./target/release/surveillance_miner config/surveillance.toml polymarket
   ```

3. **Performance**: The dashboard limits file reads for performance (checks up to 50-100 most recent files). For faster updates, adjust the refresh interval.

4. **Date Format**: Dates are in UTC by default to match how data files are stored.

5. **Keyboard Shortcuts**: The dashboard supports both arrow keys and vi-style navigation (`j`/`k`).

## Comparison with monitor.sh

| Feature | monitor.sh | dashboard.py |
|---------|-----------|--------------|
| System status | ✅ | ✅ |
| Data metrics | ✅ | ✅ |
| Market exploration | ❌ | ✅ |
| Market titles | ❌ | ✅ |
| Drill-down | ❌ | ✅ |
| Live updates | Manual | Auto-refresh |
| Interactive | No | Yes |

Use `monitor.sh` for quick status checks and scripted monitoring. Use `dashboard.py` for interactive exploration and detailed market analysis.

## Troubleshooting

### "polars not installed" error

Install polars:
```bash
pip install polars
```

Or use the virtual environment:
```bash
./setup_python_env.sh
source venv/bin/activate
./dashboard.sh
```

### "No markets found"

Run the scanner first:
```bash
./target/release/surveillance_scanner config/surveillance.toml
```

### No data showing

1. Ensure collector is running: `pgrep -f surveillance_collect`
2. Check data directory exists: `ls data/orderbook_snapshots/venue=polymarket/date=YYYY-MM-DD/`
3. Verify date is correct (use UTC date)

### Input not working (Windows)

The dashboard uses Unix terminal features. On Windows, use WSL or Git Bash for full functionality. Basic navigation with `j`/`k` should still work.

## Technical Details

- **Refresh Rate**: Default 5 seconds, configurable with `--refresh` option
- **File Reading**: Limits to most recent 50-100 Parquet files for performance
- **Data Loading**: Loads universe and stats on each refresh
- **Terminal Clearing**: Uses `clear` command (Unix) or `cls` (Windows)

## Future Enhancements

Potential improvements:
- Real-time streaming updates (WebSocket integration)
- Graph/chart visualization
- Market comparison views
- Export functionality
- Search/filter capabilities
- Curses-based UI for better terminal integration
