# Parquet File Format

## Overview

The surveillance system stores order book snapshots in Parquet format with Hive-style partitioning. Files are organized by venue, date, and hour.

## File Organization

Files are stored in the following directory structure:

```
data/orderbook_snapshots/
  venue={venue}/
    date={YYYY-MM-DD}/
      hour={HH}/
        snapshots_{YYYY-MM-DD}T{HH-mm}.parquet
```

**Example:**
```
data/orderbook_snapshots/venue=polymarket/date=2026-01-15/hour=01/snapshots_2026-01-15T01-50.parquet
```

- **Partitioning**: Hive-style partitions by `venue`, `date`, and `hour`
- **File naming**: `snapshots_{YYYY-MM-DD}T{HH-mm}.parquet` where `HH-mm` is the bucket start time (rounded down to the nearest bucket interval, default 5 minutes)
- **File rotation**: New files are created every time bucket changes (default: every 5 minutes) or when buffer reaches size limit (default: 50,000 rows)
- **Atomic writes**: Files are written to `.tmp` files first, then atomically renamed to final `.parquet` files

## Schema

Each Parquet file contains order book snapshot data with the following schema:

### Column Definitions

| Column Name | Type | Nullable | Description |
|------------|------|----------|-------------|
| `ts_recv` | Int64 | No | Timestamp when snapshot was received/created (epoch milliseconds, UTC) |
| `venue` | String (Utf8) | No | Venue identifier (e.g., "polymarket", "kalshi") |
| `market_id` | String (Utf8) | No | Market identifier (e.g., Polymarket condition ID) |
| `outcome_id` | String (Utf8) | No | Outcome identifier (e.g., "0" for NO, "1" for YES in binary markets) |
| `seq` | Int64 | No | Sequence number (monotonic per market/outcome if available, else 0) |
| `best_bid_px` | Float64 | No | Best bid price (highest bid price) |
| `best_bid_sz` | Float64 | No | Best bid size (quantity at best bid) |
| `best_ask_px` | Float64 | No | Best ask price (lowest ask price) |
| `best_ask_sz` | Float64 | No | Best ask size (quantity at best ask) |
| `mid` | Float64 | No | Mid price: (best_bid_px + best_ask_px) / 2.0, or NaN if unavailable |
| `spread` | Float64 | No | Spread: best_ask_px - best_bid_px, or NaN if unavailable |
| `bid_px` | String (Utf8) | No | **JSON-encoded array** of bid prices (sorted descending) |
| `bid_sz` | String (Utf8) | No | **JSON-encoded array** of bid sizes (corresponding to bid_px) |
| `ask_px` | String (Utf8) | No | **JSON-encoded array** of ask prices (sorted ascending) |
| `ask_sz` | String (Utf8) | No | **JSON-encoded array** of ask sizes (corresponding to ask_px) |
| `status` | String (Utf8) | No | Status: "ok", "partial", "empty", or "stale" |
| `err` | String (Utf8) | No | Error message (empty string if no error) |
| `source_ts` | Int64 | Yes (Nullable) | Source timestamp from venue (epoch milliseconds, UTC) if provided |

### Important Notes

1. **List Columns as JSON Strings**: The `bid_px`, `bid_sz`, `ask_px`, and `ask_sz` columns are currently stored as JSON-encoded strings (e.g., `"[0.5, 0.49, 0.48]"`), not as native Parquet list types. This is a temporary implementation detail and may be changed in the future to use native list types.

2. **Sorting**:
   - **Bids**: Sorted in **descending** order by price (highest bid first)
   - **Asks**: Sorted in **ascending** order by price (lowest ask first)

3. **Depth Limiting**: Only the top K levels (default: 50) are stored per side, as configured by `storage.top_k` in the config file.

4. **Status Values**:
   - `"ok"`: Both bid and ask sides have valid data
   - `"partial"`: Only one side (bid or ask) has valid data
   - `"empty"`: No valid bid or ask data
   - `"stale"`: Data is stale (future feature)

5. **NaN Values**: The `mid` and `spread` fields will be NaN if both sides aren't available (partial or empty status).

## Example Row

```json
{
  "ts_recv": 1736865600000,
  "venue": "polymarket",
  "market_id": "0x1234567890abcdef",
  "outcome_id": "1",
  "seq": 12345,
  "best_bid_px": 0.52,
  "best_bid_sz": 1000.0,
  "best_ask_px": 0.53,
  "best_ask_sz": 1500.0,
  "mid": 0.525,
  "spread": 0.01,
  "bid_px": "[0.52, 0.51, 0.50]",
  "bid_sz": "[1000.0, 2000.0, 3000.0]",
  "ask_px": "[0.53, 0.54, 0.55]",
  "ask_sz": "[1500.0, 1000.0, 2000.0]",
  "status": "ok",
  "err": "",
  "source_ts": 1736865599995
}
```

## Reading Parquet Files

### Using Polars (Rust/Python)

```python
import polars as pl

# Read a single file
df = pl.read_parquet("data/orderbook_snapshots/venue=polymarket/date=2026-01-15/hour=01/snapshots_2026-01-15T01-50.parquet")

# Parse JSON columns
df = df.with_columns([
    pl.col("bid_px").str.json_decode().alias("bid_px_list"),
    pl.col("bid_sz").str.json_decode().alias("bid_sz_list"),
    pl.col("ask_px").str.json_decode().alias("ask_px_list"),
    pl.col("ask_sz").str.json_decode().alias("ask_sz_list"),
])

# Read with partition filtering
df = pl.scan_parquet("data/orderbook_snapshots/**/*.parquet") \
    .filter(pl.col("venue") == "polymarket") \
    .filter(pl.col("ts_recv") >= 1736865600000) \
    .collect()
```

### Using PyArrow

```python
import pyarrow.parquet as pq
import pyarrow as pa
import json

# Read a single file
table = pq.read_table("data/orderbook_snapshots/venue=polymarket/date=2026-01-15/hour=01/snapshots_2026-01-15T01-50.parquet")
df = table.to_pandas()

# Parse JSON columns
df['bid_px'] = df['bid_px'].apply(json.loads)
df['bid_sz'] = df['bid_sz'].apply(json.loads)
df['ask_px'] = df['ask_px'].apply(json.loads)
df['ask_sz'] = df['ask_sz'].apply(json.loads)
```

### Using DuckDB

```sql
-- Read Parquet files
SELECT 
    ts_recv,
    venue,
    market_id,
    outcome_id,
    best_bid_px,
    best_ask_px,
    mid,
    spread,
    CAST(bid_px AS JSON) as bid_px_json,
    CAST(ask_px AS JSON) as ask_px_json
FROM 'data/orderbook_snapshots/**/*.parquet'
WHERE venue = 'polymarket'
  AND ts_recv >= 1736865600000
ORDER BY ts_recv DESC;
```

## File Characteristics

- **Format**: Parquet (Apache Parquet 2.x)
- **Compression**: Default Polars compression (typically snappy or zstd)
- **Row Groups**: Variable (optimized by Polars)
- **Encoding**: Dictionary encoding for string columns, plain encoding for numeric columns
- **File Size**: Typically 50-250 MB per file (depends on number of rows and markets)

## Configuration

File characteristics can be controlled via `config/surveillance.toml`:

```toml
[storage]
top_k = 50                    # Maximum depth levels per side
flush_rows = 50000           # Buffer size before flush
flush_seconds = 5            # Time-based flush interval (seconds)
bucket_minutes = 5           # Time bucket size (minutes)
```

## Future Improvements

1. **Native List Types**: Convert `bid_px`, `bid_sz`, `ask_px`, `ask_sz` from JSON strings to native Parquet list<f64> types
2. **Column Pruning**: Optimize schema for better column pruning during queries
3. **Statistics**: Add row group statistics for better query optimization
4. **Metadata**: Add custom metadata (e.g., schema version, writer version)
