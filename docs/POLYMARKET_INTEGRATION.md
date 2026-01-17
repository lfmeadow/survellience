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

## Data Format

### Market Info
- `market_id`: Polymarket condition ID (e.g., "0x123...")
- `outcome_ids`: Typically ["0", "1"] for binary markets
- `title`: Market question
- `close_ts`: Market end date (timestamp in milliseconds)
- `status`: "active", "closed", or "inactive"

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
