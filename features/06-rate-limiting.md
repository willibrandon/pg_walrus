# Feature: Rate Limiting

Prevent thrashing on unstable workloads by enforcing minimum time between adjustments.

## What It Does
- Enforces cooldown period between consecutive changes
- Limits maximum changes per hour
- Logs when rate limit prevents a change

## GUC Parameters
- `walrus.cooldown_sec` (int, default: 300) - Minimum seconds between changes
- `walrus.max_changes_per_hour` (int, default: 4) - Maximum adjustments per hour

## Behavior
1. Before applying any change, check rate limiter
2. If last_change + cooldown > now: skip, log "cooldown active"
3. If changes_this_hour >= max_changes_per_hour: skip, log "hourly limit reached"
4. On successful change: record timestamp, increment counter
5. Hourly counter resets after 1 hour from first change in window

## State Tracking
```rust
struct RateLimiter {
    last_change: Option<Instant>,
    changes_this_hour: u32,
    hour_start: Instant,
}
```

## Dependencies
- Requires core extension (feature 01)

## Reference
- **Core implementation**: `src/worker.rs` - Reference for adjustment trigger points
- **Feature index**: `features/README.md`
