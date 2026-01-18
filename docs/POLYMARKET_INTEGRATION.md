# Polymarket Integration

## Overview

The Polymarket venue adapter has been fully implemented with REST API and WebSocket support for market discovery and real-time order book updates.

## Features

- **Market Discovery**: Fetches active markets from Polymarket's REST API
- **WebSocket Connection**: Connects to Polymarket's WebSocket for real-time updates
- **Order Book Subscriptions**: Subscribes/unsubscribes to market/outcome pairs
- **Message Parsing**: Parses order book updates and converts to internal format

## API Endpoints

### REST API
- **Base URL**: `https://gamma-api.polymarket.com` (configurable)
- **Markets Endpoint**: `GET /markets`
  - Returns list of all available markets
  - Includes market metadata, outcomes, and status

### WebSocket
- **URL**: `wss://gamma-api.polymarket.com/ws` (configurable)
- **Channel**: `orderbook`
- **Message Format**:
  ```json
  {
    "type": "subscribe",
    "channel": "orderbook",
    "market": "0x...",
    "outcome": "0"
  }
  ```

## Configuration

In `config/surveillance.toml`:

```toml
[venues.polymarket]
enabled = true
api_key = "your_api_key"  # Optional for public endpoints
api_secret = "your_api_secret"  # Optional for public endpoints
ws_url = "wss://gamma-api.polymarket.com/ws"  # Optional, has default
rest_url = "https://gamma-api.polymarket.com"  # Optional, has default
max_subs = 200
hot_count = 40
rotation_period_secs = 180
snapshot_interval_ms_hot = 2000
snapshot_interval_ms_warm = 10000
subscription_churn_limit_per_minute = 20
```

## Usage

### Scanner
```bash
cargo run --bin surveillance_scanner
```

This will discover all Polymarket markets and write them to:
```
data/metadata/venue=polymarket/date=YYYY-MM-DD/universe.jsonl
```

### Collector
```bash
cargo run --bin surveillance_collect
```

This will:
1. Connect to Polymarket WebSocket
2. Subscribe to markets based on scheduler rotation
3. Receive order book updates
4. Write snapshots to Parquet files

## Market Structure

### Time-Boxed Markets

Polymarket offers many short-term prediction markets, particularly for crypto price movements (e.g., "Bitcoin Up or Down"). These markets have **varying time windows**:

- **5-minute windows** (e.g., "Bitcoin Up or Down - January 17, 11:35AM-11:40AM ET")
- **15-minute windows** (e.g., "XRP Up or Down - January 17, 7:00PM-7:15PM ET")
- **4-hour windows** (e.g., "Ethereum Up or Down - January 16, 4:00PM-8:00PM ET")

**Important**: Each time window is a **completely independent market** with:
- Its own unique `market_id` (condition ID)
- Its own `token_ids` (the tokens you actually trade)
- Its own `close_ts` (expiration/resolution time)

For example, there may be 400+ separate "Bitcoin Up or Down" markets in the universe file, each for a different time window.

### Market Identity

Markets with similar titles but different time windows are **NOT the same market**:

| Market ID | Title | Close TS |
|-----------|-------|----------|
| `0x036a7bf7...` | Bitcoin Up or Down - January 17, 4:45AM-5:00AM ET | 1737111600000 |
| `0xbaed8352...` | Bitcoin Up or Down - January 17, 4:15AM-4:30AM ET | 1737109800000 |

**If you place a trade on one market, it does NOT persist to another.** Each market resolves independently at the end of its time window.

### Implications for Analysis

When analyzing market data:
- **Group by `market_id`**, not by title - titles can be similar across different markets
- Each `market_id` should be analyzed separately for MM viability
- Data points from different `market_id`s should not be aggregated together, even if titles appear similar
- The `close_ts` indicates when the market resolves

## Data Format

### Market Info
- `market_id`: Polymarket condition ID (e.g., "0x123...") - **unique identifier for each market**
- `outcome_ids`: Typically ["0", "1"] for binary markets
- `title`: Market question (may be similar across different time-boxed markets)
- `close_ts`: Market end date (timestamp in milliseconds) - **when the market resolves**
- `status`: "active", "closed", or "inactive"
- `token_ids`: The actual tokens traded on the CLOB (one per outcome)

### Order Book Updates
- `market_id`: Condition ID
- `outcome_id`: Outcome ID ("0" or "1")
- `bids`: Array of [price, size] pairs
- `asks`: Array of [price, size] pairs
- `timestamp_ms`: Update timestamp
- `sequence`: Monotonic sequence number

## Dependencies

The integration requires:
- `reqwest` with `rustls-tls` feature (for REST API)
- `tokio-tungstenite` (for WebSocket)
- `serde` and `serde_json` (for message parsing)

**Note**: If you encounter OpenSSL build errors, you may need to:
1. Install OpenSSL development packages:
   - Ubuntu/Debian: `sudo apt-get install libssl-dev pkg-config`
   - Fedora: `sudo dnf install openssl-devel pkg-config`
2. Or use rustls features (already configured) which should avoid OpenSSL

## Testing

Run tests:
```bash
cargo test --lib venue::polymarket
```

## Error Handling

The implementation includes:
- Connection retry logic (via tokio-tungstenite)
- Message parsing error handling
- WebSocket reconnection support
- Graceful handling of missing fields in API responses

## Limitations

1. **Authentication**: Currently uses public endpoints. Private endpoints requiring authentication are not yet implemented.
2. **Rate Limiting**: No explicit rate limiting - relies on Polymarket's API limits
3. **Reconnection**: Basic reconnection - could be enhanced with exponential backoff

## Future Enhancements

- [ ] Add authentication for private endpoints
- [ ] Implement exponential backoff for reconnections
- [ ] Add rate limiting
- [ ] Support for multi-outcome markets beyond binary
- [ ] Add metrics and monitoring
