# Feature: WAL Generation Rate Tracking

Monitor actual WAL generation rate for predictive sizing before forced checkpoints occur.

## What It Does
- Tracks WAL LSN position between analysis cycles
- Calculates MB/sec generation rate
- Maintains rolling window of rate samples
- Predicts size needed based on 95th percentile rate

## Implementation
```rust
struct WalRateTracker {
    last_lsn: Option<XLogRecPtr>,
    last_time: Option<Instant>,
    rate_samples: VecDeque<f64>,  // MB/sec
    max_samples: usize,           // e.g., 100
}
```

## Calculation
1. Get current LSN via `GetXLogWriteRecPtr()`
2. Calculate bytes written since last check
3. Derive MB/sec rate
4. Store in rolling sample buffer
5. Predict: `p95_rate * checkpoint_timeout * 1.3` (30% headroom)

## Integration Points
- Used by Adaptive and Percentile algorithms (feature 09)
- Can trigger proactive resize BEFORE forced checkpoint
- `walrus.proactive_sizing` (bool, default: false) - Enable proactive mode

## SQL Function
`walrus_wal_rate() -> JSONB`
- current_rate_mb_sec, avg_rate, p95_rate, samples_count

## Dependencies
- Requires core extension (feature 01)
- Enhances smart algorithms (feature 09)

## Reference
- **pg_walsizer source**: `pg_walsizer/walsizer.c` - Reference for checkpoint stats access
- **Enhancements design**: `ENHANCEMENTS_PROPOSAL.md` - Section 4.2 WAL Rate Tracking
- **Feature index**: `features/README.md`
