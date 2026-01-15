# Kalshi Integration

## Overview

The Kalshi venue adapter is structured and ready for implementation. Kalshi uses RSA-PSS signature authentication, which requires additional cryptographic handling compared to simple API key/secret authentication.

## Authentication

Kalshi uses RSA-PSS signature-based authentication:

1. **Access Key ID**: Your Kalshi API access key
   - Obtain from: https://trade.kalshi.com/trade-api/account/settings
   - Navigate to Account Settings > API Keys > Create New API Key

2. **Private Key**: RSA private key in PEM format
   - Provided when creating the API key
   - Must be stored securely (never commit to version control)
   - Can be provided as:
     - Full PEM content in `api_secret` field
     - Path to `.pem` file (future enhancement)

3. **Request Signing**: Each API request requires:
   - `KALSHI-ACCESS-KEY`: Access Key ID
   - `KALSHI-ACCESS-TIMESTAMP`: Current timestamp in milliseconds
   - `KALSHI-ACCESS-SIGNATURE`: RSA-PSS signature of `timestamp + HTTP method + path`

## API Endpoints

### REST API
- **Base URL**: `https://api.kalshi.com/trade-api/v2` (configurable)
- **Markets Endpoint**: `GET /markets`
  - Requires authentication
  - Returns list of available markets

### WebSocket
- **URL**: `wss://api.kalshi.com/trade-api/v2/ws` (configurable)
- **Authentication**: Required via initial auth message
- **Message Format**: TBD (needs implementation)

## Configuration

In `config/surveillance.toml`:

```toml
[venues.kalshi]
enabled = true
api_key = "your-access-key-id"  # Kalshi Access Key ID (or leave empty to load from ~/.ssh/kalshi)
api_secret = "-----BEGIN RSA PRIVATE KEY-----\n...\n-----END RSA PRIVATE KEY-----"  # RSA private key content (or leave empty to load from ~/.ssh/id_kalshi_rsa)
ws_url = "wss://api.kalshi.com/trade-api/v2/ws"  # Optional, has default
rest_url = "https://api.kalshi.com/trade-api/v2"  # Optional, has default
max_subs = 200
hot_count = 40
rotation_period_secs = 180
snapshot_interval_ms_hot = 2000
snapshot_interval_ms_warm = 10000
subscription_churn_limit_per_minute = 20
```

### Default File Paths

If `api_key` or `api_secret` are empty in the config, the system will automatically try to load credentials from:

- **Access Key ID**: `~/.ssh/kalshi`
- **Private Key**: `~/.ssh/id_kalshi_rsa`

This allows you to keep credentials out of the config file entirely:

```toml
[venues.kalshi]
enabled = true
api_key = ""  # Will load from ~/.ssh/kalshi
api_secret = ""  # Will load from ~/.ssh/id_kalshi_rsa
```

Make sure these files exist and have appropriate permissions:
```bash
chmod 600 ~/.ssh/kalshi ~/.ssh/id_kalshi_rsa
```

## Implementation Status

### ✅ Completed
- Venue adapter structure
- Configuration support
- Integration points in scanner and collector binaries

### 🚧 TODO
- [ ] Implement RSA-PSS signature generation for REST API calls
- [ ] Implement WebSocket authentication
- [ ] Implement market discovery REST API call
- [ ] Implement WebSocket connection and message handling
- [ ] Implement order book subscription/unsubscription
- [ ] Parse Kalshi order book message format
- [ ] Map Kalshi market/outcome structure to internal `MarketInfo` format

## Security Notes

- **Never commit credentials**: Use environment variables or secure secret management
- **Private key storage**: Consider using environment variables for the RSA private key:
  ```bash
  export KALSHI_PRIVATE_KEY="$(cat ~/.kalshi/private_key.pem)"
  ```
- **Key rotation**: Rotate API keys regularly
- **Demo environment**: Test in Kalshi's demo environment before production

## Resources

- [Kalshi API Documentation](https://docs.kalshi.com/)
- [Getting Started with API Keys](https://docs.kalshi.com/getting_started/api_keys)
- [Kalshi Python SDK](https://github.com/Kalshi/kalshi-python) (reference implementation)

## Example: Using Environment Variables

For better security, consider loading credentials from environment variables:

```toml
[venues.kalshi]
enabled = true
api_key = "${KALSHI_ACCESS_KEY_ID}"  # Would need env var expansion support
api_secret = "${KALSHI_PRIVATE_KEY}"  # Would need env var expansion support
```

Or modify the code to read from environment variables if config values are empty.
