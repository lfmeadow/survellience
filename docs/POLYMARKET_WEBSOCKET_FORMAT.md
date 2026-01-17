# Polymarket CLOB WebSocket Message Format Analysis

## Documentation vs. Reality

### Official Documentation (from docs.polymarket.com)

According to Polymarket's RTDS (Real-Time Data Stream) documentation:
- **Endpoint**: `wss://ws-subscriptions-clob.polymarket.com/ws/market`
- **Subscription**: `{"type": "market", "assets_ids": ["<token_id>"]}`
- **Message Format**: Documentation mentions messages include `topic`, `type`, `timestamp`, and `payload` fields

**However**, the actual messages received do NOT match this documented format.

### Actual Message Formats Received

The Polymarket CLOB WebSocket (`wss://ws-subscriptions-clob.polymarket.com/ws/market`) uses a different message format than documented.

## Actual Message Formats Received

### Format 1: Order Book Snapshot
```json
{
  "market": "0x8499755b2b439aa3da94a16b470803038a276954895957cb3cfaf417e3654179",
  "asset_id": "41236383224347298420125708292657012871464205363068451527890025588992975750774",
  "timestamp": "1768422554971",
  "hash": "2f7b8dfcf819871167071fcd669bd03c3d9c98a3",
  "bids": [
    {"price": "0.01", "size": "473"},
    {"price": "0.02", "size": "55000"}
  ],
  "asks": [
    {"price": "0.99", "size": "100"},
    {"price": "0.98", "size": "200"}
  ]
}
```

**Key differences from expected:**
- Uses `asset_id` (token ID) instead of `outcome` field
- `bids`/`asks` are arrays of objects `{"price": "...", "size": "..."}` not arrays of arrays `[[price, size], ...]`
- Prices and sizes are strings, not numbers
- No `type` or `message_type` field
- No `outcome` field

### Format 2: Price Changes (Incremental Updates)
```json
{
  "market": "0xec051d57c8dfd981cdd5f3113b9f7311fa5b90a3ec4eb8bd3d37a6ce4465f186",
  "price_changes": [
    {
      "asset_id": "4277044238652662163120456541739746435744245249534924848626026418182722376831",
      "price": "0.38",
      "size": "20",
      "side": "BUY",
      "hash": "dd4d38b5612c46e4310b6998b679390d83017b72",
      "best_bid": "0.4",
      "best_ask": "0.41"
    }
  ]
}
```

**Key differences:**
- Uses `price_changes` array instead of direct `bids`/`asks`
- Each change has `asset_id`, `side` (BUY/SELL), `price`, `size`
- Includes `best_bid` and `best_ask` in each change
- No `outcome` field - uses `asset_id` (token ID) instead

### Format 3: Trade Events
```json
{
  "market": "0x8499755b2b439aa3da94a16b470803038a276954895957cb3cfaf417e3654179",
  "asset_id": "41236383224347298420125708292657012871464205363068451527890025588992975750774",
  "price": "0.82",
  "size": "1.280483",
  "fee_rate_bps": "1000",
  "side": "BUY",
  "timestamp": "1768422910295",
  "event_type": "last_trade_price",
  "transaction_hash": "0x13585d83601e1c4408aa5906087a91f37eb5c254bc51397bd8891486c8ad0c75"
}
```

## Current Code Expectations

The code expects (from `PolymarketOrderBookMessage`):
```rust
struct PolymarketOrderBookMessage {
    message_type: String,        // Expected: "orderbook" | "update"
    market: Option<String>,       // ✅ Present
    outcome: Option<String>,      // ❌ NOT present - uses asset_id instead
    bids: Option<Vec<[f64; 2]>>, // ❌ Wrong format - expects [[price, size], ...]
    asks: Option<Vec<[f64; 2]>>, // ❌ Wrong format - expects [[price, size], ...]
    timestamp: Option<i64>,      // ✅ Present (but as string)
    sequence: Option<i64>,       // ❌ NOT present
}
```

## Issues Identified

1. **Missing `outcome` field**: Messages use `asset_id` (token ID) instead of `outcome`
2. **Wrong `bids`/`asks` format**: 
   - Expected: `Vec<[f64; 2]>` (array of arrays: `[[0.5, 100.0], [0.49, 200.0]]`)
   - Actual: Array of objects: `[{"price": "0.5", "size": "100.0"}, ...]`
3. **String vs Number**: Prices and sizes are strings, not numbers
4. **No `message_type`**: Messages don't have a `type` field to distinguish formats
5. **Multiple message types**: 
   - Order book snapshots (with `bids`/`asks`)
   - Price changes (with `price_changes`)
   - Trade events (with `event_type`)
6. **No `sequence` field**: Messages don't include sequence numbers

## Mapping Strategy

To map CLOB messages to our internal format:

1. **Market/Outcome mapping**: 
   - `asset_id` (token ID) → Need to map back to `(market_id, outcome_id)`
   - We subscribed using token IDs, so we need to maintain a mapping
   - Token ID → (market_id, outcome_id) mapping from universe file

2. **Order book reconstruction**:
   - For snapshots: Parse `bids`/`asks` arrays of objects
   - For price changes: Reconstruct order book from `price_changes` array
   - Convert string prices/sizes to f64

3. **Message type detection**:
   - Has `bids`/`asks` → Order book snapshot
   - Has `price_changes` → Incremental update
   - Has `event_type` → Trade event (can be ignored for order book)

## Documentation Discrepancy

**Key Finding**: The actual message format does NOT match the documented RTDS format.

**Documented Format** (from Polymarket docs):
- Messages should have `topic`, `type`, `timestamp`, `payload` structure
- `payload` should contain event-specific data

**Actual Format** (observed):
- Messages are direct JSON objects (no `topic`/`payload` wrapper)
- Three distinct message types with different structures
- No `type` field to distinguish message types
- Uses `asset_id` (token ID) instead of `outcome` field

## Code vs. Reality Comparison

### Current Code Expectations (`PolymarketOrderBookMessage`):
```rust
struct PolymarketOrderBookMessage {
    message_type: String,        // ❌ NOT in actual messages
    market: Option<String>,      // ✅ Present
    outcome: Option<String>,      // ❌ NOT present - uses asset_id instead
    bids: Option<Vec<[f64; 2]>>, // ❌ Wrong: expects [[price, size], ...]
    asks: Option<Vec<[f64; 2]>>, // ❌ Wrong: expects [[price, size], ...]
    timestamp: Option<i64>,      // ⚠️ Present but as string
    sequence: Option<i64>,       // ❌ NOT present
}
```

### Actual Message Format 1: Order Book Snapshot
```json
{
  "market": "0x...",           // ✅ Matches
  "asset_id": "123...",        // ❌ Code expects "outcome"
  "timestamp": "1768422554971", // ⚠️ String, not i64
  "hash": "...",
  "bids": [                     // ❌ Array of objects, not array of arrays
    {"price": "0.01", "size": "473"}
  ],
  "asks": [                     // ❌ Array of objects, not array of arrays
    {"price": "0.99", "size": "100"}
  ]
}
```

### Actual Message Format 2: Price Changes (Incremental)
```json
{
  "market": "0x...",
  "price_changes": [            // ❌ Code doesn't handle this
    {
      "asset_id": "123...",
      "price": "0.38",
      "size": "20",
      "side": "BUY",
      "best_bid": "0.4",
      "best_ask": "0.41"
    }
  ]
}
```

### Actual Message Format 3: Trade Events
```json
{
  "market": "0x...",
  "asset_id": "123...",
  "event_type": "last_trade_price",  // ❌ Code doesn't handle
  "price": "0.82",
  "size": "1.280483",
  "side": "BUY",
  "timestamp": "1768422910295"
}
```

## Critical Issues

1. **No `outcome` field**: Messages use `asset_id` (token ID), requiring reverse mapping
2. **Wrong `bids`/`asks` structure**: 
   - Expected: `[[0.5, 100.0], [0.49, 200.0]]`
   - Actual: `[{"price": "0.5", "size": "100.0"}, ...]`
3. **String vs Number**: All prices/sizes are strings, need parsing
4. **No `message_type` field**: Can't distinguish message types without structure inspection
5. **Multiple formats**: Three different message structures (snapshot, price_changes, trade)
6. **No `sequence` field**: Must generate sequence numbers ourselves
7. **Array messages**: Sometimes messages come as arrays `[{...}, {...}]` not single objects

## Recommendations

1. **Create new structs** for actual CLOB message formats:
   - `PolymarketClobOrderBookSnapshot` (with `bids`/`asks` as objects)
   - `PolymarketClobPriceChanges` (with `price_changes` array)
   - `PolymarketClobTradeEvent` (with `event_type`)

2. **Maintain token_id → (market_id, outcome_id) mapping**:
   - Load from universe.jsonl file
   - Map `asset_id` from messages to `(market_id, outcome_id)` pairs

3. **Handle all three message types**:
   - Parse order book snapshots directly
   - Reconstruct order book from `price_changes` (incremental updates)
   - Ignore trade events (or use for validation)

4. **Convert string prices/sizes to f64**:
   - Parse `"0.5"` → `0.5f64`
   - Handle potential parsing errors

5. **Handle array messages**: Check if message is array and process each element

6. **Generate sequence numbers**: Since messages don't include sequence, use timestamp or internal counter
