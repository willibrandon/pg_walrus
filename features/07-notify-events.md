# Feature: NOTIFY/LISTEN Integration

Real-time PostgreSQL notifications when sizing adjustments occur.

## What It Does
- Sends NOTIFY on configurable channel when adjustments happen
- Payload is JSON with event details
- Allows real-time monitoring without polling

## GUC Parameters
- `walrus.notify_channel` (string, default: 'walrus_events') - Channel name, empty to disable

## Event Payload
```json
{
  "event": "adjustment",
  "action": "increase",
  "old_size_mb": 1024,
  "new_size_mb": 2048,
  "forced_checkpoints": 5,
  "timestamp": "2025-12-29T10:30:00Z"
}
```

## Client Usage
```sql
LISTEN walrus_events;
-- Receive notifications asynchronously
```

## Implementation
- Use SPI to execute `NOTIFY channel, 'payload'`
- Escape single quotes in JSON payload
- Send after successful change (not on dry-run unless configured)

## Dependencies
- Requires core extension (feature 01)

## Reference
- Feature index: `features/README.md`
