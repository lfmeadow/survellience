# Sequence Gap Detection Fix

## Problem

The collector was generating false sequence gap warnings because:

1. **Global sequence counter**: All markets/outcomes shared a single global `AtomicU64` sequence counter
2. **Per-market/outcome gap tracking**: Gap detection tracked sequences per `(market_id, outcome_id)` pair
3. **False positives**: When Market A got sequence 100, Market B got 101, Market A got 102, the tracker for Market A incorrectly thought sequence 101 was dropped

Example false warning:
```
WARN Sequence gap detected: market=0x..., outcome=0, expected=161992, got=162016, gap=24
```

## Root Cause

Polymarket CLOB WebSocket messages don't provide sequence numbers. We generate our own, but were using a global counter across all markets, making gap detection meaningless.

## Solution

Implemented **per-market/outcome sequence tracking**:

1. Added `market_sequences: Arc<Mutex<HashMap<(String, String), AtomicU64>>>` to track sequences per `(market_id, outcome_id)` pair
2. Updated all three CLOB WebSocket message parsing locations to use per-market/outcome counters:
   - Order book snapshots
   - Price changes
   - Array of snapshots

## Changes Made

### `services/surveillance/src/venue/polymarket.rs`

1. Added `market_sequences` field to `PolymarketVenue` struct
2. Initialized `market_sequences` in `new()` method
3. Updated sequence generation in three locations:
   - Order book snapshot parsing (line ~621)
   - Price change parsing (line ~693)
   - Array snapshot parsing (line ~761)

Each location now:
- Gets a per-market/outcome counter from the HashMap
- Creates a new counter (starting at 1) if it doesn't exist
- Increments the counter atomically
- Uses that counter value for the sequence number

## Result

- ✅ Sequence numbers are now tracked per market/outcome pair
- ✅ Gap detection will only warn about actual drops (if Polymarket provides sequences in the future)
- ✅ False warnings should stop after restart

## Note

Since Polymarket CLOB messages don't provide sequence numbers, we can't detect actual message drops from the WebSocket. The per-market/outcome counters ensure sequences are monotonic per market/outcome, which makes gap detection meaningful if/when Polymarket adds sequence numbers to their messages.

## Testing

After rebuilding and restarting the collector:

```bash
cargo build --release
sudo systemctl restart surveillance-collect
sudo journalctl -u surveillance-collect -f | grep "Sequence gap"
```

You should see significantly fewer (ideally zero) gap warnings. Any remaining gaps would indicate actual message drops if Polymarket provides sequence numbers, or would need investigation.
