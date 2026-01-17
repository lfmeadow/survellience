# Process Lifecycle and Architecture

## Overview

The surveillance system consists of **3 independent processes** that work together to discover, collect, and analyze market data. They are designed to run independently and coordinate through file-based communication.

## Processes

### 1. Scanner (`surveillance_scanner`)

**Purpose**: Discover and catalog available markets from venues (Polymarket, Kalshi)

**Entry Point**: `bins/surveillance_scanner.rs::main()`

**Lifecycle**:
- **Start**: Manual execution (via CLI)
- **Run**: 
  - Loads configuration from `config/surveillance.toml`
  - Initializes venue adapters (Polymarket, Kalshi, or Mock)
  - Calls `venue.discover_markets()` for each enabled venue
  - Writes market metadata to `data/metadata/venue={venue}/date={YYYY-MM-DD}/universe.jsonl`
- **Exit**: Completes after writing all universe files (typically runs for seconds to minutes)

**When to Run**:
- **Initial setup**: Once to create the initial market universe
- **Periodic refresh**: Daily (or as needed) to discover new markets and update existing ones
- **Before starting collector**: Recommended to ensure fresh market data

**Command**:
```bash
./target/release/surveillance_scanner config/surveillance.toml
```

**Output Files**:
- `data/metadata/venue={venue}/date={YYYY-MM-DD}/universe.jsonl` - One file per venue per day

**Dependencies**: None (can run independently)

---

### 2. Collector (`surveillance_collect`)

**Purpose**: Continuously collect order book snapshots from venues via WebSocket

**Entry Point**: `bins/surveillance_collect.rs::main()`

**Lifecycle**:
- **Start**: Manual execution or systemd service
- **Initialization**:
  - Loads configuration from `config/surveillance.toml`
  - Creates shared components:
    - `ParquetWriter` (for writing snapshots to disk)
    - `Scheduler` (embedded, not a separate process) - manages subscription rotation
    - `Collector` instance(s) - one per enabled venue
  - Spawns one async task per venue collector
- **Runtime** (long-running, continuous):
  - **WebSocket Connection**: Connects to venue WebSocket (e.g., Polymarket CLOB WebSocket)
  - **Subscription Management**: Manages WebSocket subscriptions based on scheduler decisions
  - **Message Processing**: Receives order book updates, maintains in-memory book store
  - **Snapshot Generation**: Periodically takes snapshots of order book state
  - **Parquet Writing**: Writes snapshots to Parquet files in Hive-style partitions
  - **Rotation**: Every `rotation_period_secs` (default: 180s), scheduler selects new markets to subscribe to
- **Exit**: Only on error or manual termination (Ctrl+C, kill signal, systemd stop)

**When to Run**:
- **Continuous**: Should run 24/7 for continuous data collection
- **Background**: Use `nohup`, `systemd`, or similar for long-running operation

**Command**:
```bash
# Foreground (for testing)
./target/release/surveillance_collect config/surveillance.toml

# Background (production)
nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &

# Systemd (production)
sudo systemctl start surveillance-collect
```

**Output Files**:
- `data/orderbook_snapshots/venue={venue}/date={YYYY-MM-DD}/hour={HH}/snapshots_{timestamp}.parquet` - Multiple files per hour (every 5 minutes by default)

**Dependencies**: 
- **Requires**: Scanner must have run first to create universe files (collector reads these for subscription selection)
- **Optional**: Miner stats cache (if available, scheduler uses it for better market scoring)

**Internal Components** (all run within the collector process):
- **Scheduler** (embedded): Not a separate process! Runs as part of the collector
  - Decides which markets to subscribe to (hot vs warm)
  - Manages subscription rotation
  - Loads universe files and stats cache
- **SubscriptionManager**: Manages WebSocket subscriptions (subscribe/unsubscribe)
- **BookStore**: In-memory storage of current order book state
- **Snapshotter**: Takes periodic snapshots of order book state
- **ParquetWriter**: Writes snapshots to disk

---

### 3. Miner (`surveillance_miner`)

**Purpose**: Analyze collected order book snapshots and generate statistics

**Entry Point**: `bins/surveillance_miner.rs::main()`

**Lifecycle**:
- **Start**: Manual execution (via CLI, optionally via cron)
- **Run**:
  - Loads configuration from `config/surveillance.toml`
  - Reads Parquet snapshots from `data/orderbook_snapshots/venue={venue}/date={date}/`
  - Computes statistics (spread, depth, volume, etc.) per market
  - Writes stats cache to `data/stats/venue={venue}/date={date}/stats.parquet`
  - Prints summary of top markets
- **Exit**: Completes after processing all data (typically runs for seconds to minutes)

**When to Run**:
- **Periodic**: Hourly or daily to analyze collected data
- **On-demand**: When you need to analyze specific dates
- **After collection**: Run after collector has gathered data

**Command**:
```bash
# Analyze specific date
./target/release/surveillance_miner config/surveillance.toml polymarket 2026-01-14

# Analyze today's data
./target/release/surveillance_miner config/surveillance.toml polymarket $(date +%Y-%m-%d)

# Analyze all dates (script needed)
```

**Output Files**:
- `data/stats/venue={venue}/date={YYYY-MM-DD}/stats.parquet` - One file per venue per day

**Dependencies**: 
- **Requires**: Collector must have run first to create Parquet snapshot files

---

## Architecture: How They Work Together

```
┌─────────────────┐
│   Scanner       │  (runs periodically, e.g., daily)
│                 │
│  Discovers      │
│  markets from   │
│  venues         │
└────────┬────────┘
         │
         │ writes
         ▼
┌─────────────────┐
│ universe.jsonl  │  (market metadata)
│ (per venue/day) │
└────────┬────────┘
         │
         │ read by
         ▼
┌─────────────────┐
│   Collector     │  (runs continuously)
│                 │
│  ├─ Scheduler   │  ← reads universe.jsonl
│  │  (embedded)  │     selects markets to subscribe
│  │              │
│  ├─ Sub Manager │  ← subscribes to markets via WebSocket
│  │              │
│  ├─ Book Store  │  ← receives & stores order book updates
│  │              │
│  ├─ Snapshotter │  ← takes periodic snapshots
│  │              │
│  └─ Writer      │  ← writes snapshots to Parquet
└────────┬────────┘
         │
         │ writes
         ▼
┌─────────────────┐
│ snapshot files  │  (order book snapshots)
│ (Parquet, many) │
└────────┬────────┘
         │
         │ read by
         ▼
┌─────────────────┐
│    Miner        │  (runs periodically, e.g., hourly/daily)
│                 │
│  Reads snapshots│
│  Computes stats │
│  Writes cache   │
└────────┬────────┘
         │
         │ writes (optional, used by scheduler)
         ▼
┌─────────────────┐
│  stats.parquet  │  (market statistics cache)
│  (per venue/day)│
└─────────────────┘
```

## Key Points

1. **Scanner is Independent**: Can run anytime, creates universe files that collector reads
2. **Collector is Long-Running**: Should run continuously, contains scheduler as embedded component
3. **Scheduler is NOT a Separate Process**: It's embedded in the collector, runs as part of the collector's main loop
4. **Miner is Independent**: Can run anytime to analyze collected data
5. **File-Based Coordination**: Processes coordinate through files (universe.jsonl, snapshot files, stats.parquet)
6. **No Direct IPC**: Processes don't communicate directly (no pipes, sockets, etc.)

## Typical Workflow

1. **Initial Setup**:
   ```bash
   # 1. Discover markets
   ./target/release/surveillance_scanner config/surveillance.toml
   
   # 2. Start collecting data (runs continuously)
   nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &
   ```

2. **Ongoing Operations**:
   ```bash
   # Daily: Refresh market universe
   ./target/release/surveillance_scanner config/surveillance.toml
   
   # Collector keeps running (restart only if needed)
   # Monitor: tail -f collector.log
   
   # Periodic: Analyze collected data
   ./target/release/surveillance_miner config/surveillance.toml polymarket $(date +%Y-%m-%d)
   ```

3. **Monitoring**:
   ```bash
   # Check if collector is running
   pgrep -f surveillance_collect
   
   # Check recent data files
   find data/orderbook_snapshots -name "*.parquet" -mmin -10
   
   # Check logs
   tail -f collector.log
   ```

## Process Management

### Manual Management

- **Start Collector**: `nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &`
- **Stop Collector**: `pkill -f surveillance_collect`
- **Restart Collector**: `pkill -f surveillance_collect && nohup ./target/release/surveillance_collect config/surveillance.toml > collector.log 2>&1 &`

### Systemd (Recommended for Production)

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

### Cron (For Scanner and Miner)

Add to crontab (`crontab -e`):
```bash
# Daily market discovery at 2 AM
0 2 * * * /path/to/survellience/target/release/surveillance_scanner /path/to/survellience/config/surveillance.toml

# Hourly data analysis
0 * * * * /path/to/survellience/target/release/surveillance_miner /path/to/survellience/config/surveillance.toml polymarket $(date +\%Y-\%m-\%d)
```

## Summary

- **Scanner**: Run periodically (daily), exits after completion
- **Collector**: Run continuously (24/7), contains embedded scheduler
- **Miner**: Run periodically (hourly/daily), exits after completion
- **Scheduler**: NOT a separate process - embedded in collector
- **Coordination**: File-based (universe.jsonl, Parquet files, stats.parquet)
- **Lifecycle**: Scanner → Collector → Miner (each can run independently)
