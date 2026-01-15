## Current Status (2026-01-14)

### What was added / changed recently
- Added MM viability analysis module: `services/surveillance/src/analytics/mm_viability.rs`.
- Wired CLI subcommand: `surveillance_miner mm-viability --venue ... --date ... --hours ...`.
- Updated `README.md` with new miner usage and mm-viability example.
- Scheduler now rotates WARM markets and keeps HOT stable unless scores change:
  - HOT = 10% of `max_subs` (minimum 1), WARM rotates via cursor.
- Stats cache loading implemented in scheduler (`stats.parquet` aggregated per market).
- Trade events parsing for Polymarket CLOB:
  - Last-trade events handled from both `event_type` or `type`.
  - Trades are buffered and written to Parquet in `data/trades/venue=polymarket/date=YYYY-MM-DD/hour=HH/trades_YYYY-MM-DDTHH-mm.parquet`.
  - Log message: `Trade events seen in last 60s: N`.
- Fixed WS queue ordering (VecDeque + FIFO) to avoid false sequence-gap warnings.
- Metrics fixes: queue depth no longer underflows and msg_rate tracks correctly.
- Config changes (Polymarket):
  - `max_subs = 500`
  - `hot_count` now ignored by scheduler (HOT=10% of max_subs)
  - `rotation_period_secs = 30`
  - `subscription_churn_limit_per_minute = 3`

### Known behaviors / caveats
- HOT set only changes when scores change (universe/stats cache). Rotation only changes WARM.
- `stats.parquet` must exist (run miner) or scores remain static.
- `mm-viability` report returns empty if data volume is low:
  - Default filter: `min_rows >= 1000` per (market_id,outcome_id).
  - Example run for 2026-01-14 returned empty because only 3 parquet files existed.

### How to run mm-viability
```bash
cargo run -p surveillance --bin surveillance_miner -- mm-viability \
  --venue polymarket --date 2026-01-14 --hours all \
  --fee-estimate 0.0 --top 20 --write-report=true
```
Output: `data/reports/mm_viability/venue=polymarket/date=YYYY-MM-DD/mm_viability.parquet`.

### Trade parquet monitoring
```bash
./monitor.sh polymarket
```
Shows “TRADE DATA” section and latest trade parquet if present.

### Pending follow-ups
- If mm-viability output is empty, lower `min_rows` or run on a date with more data.
- If no trade parquet files appear, confirm trade event availability in CLOB feed.

## New Chat Prompt

You are a senior Rust engineer joining the surveillance project. Please read `PROJECT_STATUS.md` first, then:
1) Verify the current build (`cargo check`) and confirm mm-viability CLI works.
2) Run mm-viability for a date with sufficient data; if empty, lower min_rows or add CLI flag.
3) Confirm trade parquet capture for Polymarket CLOB (or verify last_trade_price availability).
4) Make sure scheduler rotation changes WARM and HOT is stable unless scores change.
