# WebSocket Monitoring and Drop Detection

## Overview

The collector now tracks WebSocket message rates and detects dropped messages through sequence gap detection. This allows you to monitor:

1. **Incoming Message Rate**: Messages per second being processed
2. **Drop Detection**: Sequence gaps indicating lost WebSocket messages
3. **Queue Depth**: Current backlog of unprocessed messages
4. **Error Tracking**: Total errors encountered

## Metrics Tracked

### Message Rate
- **Updates per second**: Rate at which order book updates are processed
- Reported every 60 seconds in logs

### Sequence Gap Detection
- Tracks sequence numbers per market/outcome pair
- Detects gaps when expected sequence ≠ received sequence
- Logs warnings when gaps are detected
- Tracks total gaps and out-of-order messages per market

### Queue Depth
- Tracks the depth of the message queue
- Updated as messages are received and processed
- Included in metrics reports

### Error Tracking
- Counts WebSocket receive errors
- Included in metrics reports

## Metrics Reporting

Metrics are automatically logged every 60 seconds with the following format:

```
INFO WebSocket metrics: msg_rate=X.X/s, update_rate=X.X/s, queue_depth=X, total_msg=X, total_updates=X, errors=X, gaps=X, out_of_order=X, markets_with_issues=X
```

### Metrics Fields

- **msg_rate**: Messages per second (when message arrival is tracked)
- **update_rate**: Updates processed per second
- **queue_depth**: Current message queue depth
- **total_msg**: Total messages received (when tracked)
- **total_updates**: Total updates processed
- **errors**: Total WebSocket errors
- **gaps**: Total sequence gaps detected across all markets
- **out_of_order**: Total out-of-order messages detected
- **markets_with_issues**: Number of markets with detected gaps or out-of-order messages

## Sequence Gap Detection

### How It Works

1. Each order book update includes a `sequence` number
2. The collector tracks the last sequence number for each (market_id, outcome_id) pair
3. When a new update arrives:
   - If `new_sequence == last_sequence + 1`: Normal (no gap)
   - If `new_sequence > last_sequence + 1`: Gap detected (messages dropped)
   - If `new_sequence < last_sequence`: Out-of-order message

### Gap Detection Warnings

When a gap is detected, you'll see a warning like:

```
WARN Sequence gap detected: market=0x123..., outcome=0, expected=42, got=45, gap=3, total_gaps=5
```

This indicates:
- Market ID and outcome ID
- Expected sequence number (last + 1)
- Actual sequence number received
- Gap size (how many messages were skipped)
- Total gaps detected for this market/outcome

## Monitoring WebSocket Performance

### View Metrics in Logs

The collector logs metrics every 60 seconds. To view them:

```bash
# If running via systemd
sudo journalctl -u surveillance-collect -f | grep "WebSocket metrics"

# If running directly
tail -f collector.log | grep "WebSocket metrics"
```

### Check for Drops

To check if messages are being dropped:

```bash
# Look for gap warnings
sudo journalctl -u surveillance-collect | grep "Sequence gap detected"

# Count total gaps in logs
sudo journalctl -u surveillance-collect | grep "Sequence gap detected" | wc -l
```

### Monitor Message Rate

```bash
# View recent metrics
sudo journalctl -u surveillance-collect -n 20 | grep "WebSocket metrics"

# Extract just the rates
sudo journalctl -u surveillance-collect | grep "WebSocket metrics" | tail -5
```

## Current Limitations

1. **Message Arrival Tracking**: Currently tracks processing rate, not raw WebSocket message arrival rate. The actual queue depth tracking needs improvement.

2. **Gap Resolution**: When gaps are detected, the collector doesn't automatically recover or request missed messages. It just logs the gap.

3. **Sequence Number Source**: Sequence numbers come from the WebSocket messages themselves. If the venue doesn't provide reliable sequence numbers, gap detection may not work correctly.

4. **Queue Depth**: Queue depth is tracked based on receive/process events, but may not reflect the actual venue queue depth accurately.

## Future Enhancements

Potential improvements:

1. **Expose Metrics via API**: Provide HTTP endpoint to query current metrics
2. **Prometheus Integration**: Export metrics in Prometheus format
3. **Alerting**: Alert when message rate drops or gap count exceeds threshold
4. **Recovery**: Implement gap recovery mechanisms (re-subscribe, request replay)
5. **Real-time Dashboard**: Add metrics to the live dashboard

## Configuration

Metrics reporting interval can be adjusted in the collector code:

```rust
let metrics = Arc::new(WebSocketMetrics::new(60)); // 60 seconds
```

Change the value to adjust reporting frequency.

## Example Output

```
INFO WebSocket metrics: msg_rate=0.0/s, update_rate=15.3/s, queue_depth=2, total_msg=0, total_updates=456, errors=0, gaps=3, out_of_order=0, markets_with_issues=2

WARN Sequence gap detected: market=0x8213d395e079614d6c4d7f4cbb9be9337ab51648a21cc2a334ae8f1966d164b4, outcome=0, expected=123, got=125, gap=2, total_gaps=1
```

This shows:
- Processing 15.3 updates/second
- Queue depth of 2 messages
- Processed 456 total updates
- 3 sequence gaps detected across 2 markets
- A specific gap warning for one market (expected 123, got 125, missing 1 message)
