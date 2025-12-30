# Feature: Prometheus Metrics Export

Expose extension metrics in Prometheus format for standard monitoring integration.

## SQL Function
`walrus_metrics_prometheus() -> TEXT`

## Metrics Exposed
```
# HELP walrus_max_wal_size_bytes Current max_wal_size setting
# TYPE walrus_max_wal_size_bytes gauge
walrus_max_wal_size_bytes {} 2147483648

# HELP walrus_max_allowed_bytes Maximum allowed by walrus.max
# TYPE walrus_max_allowed_bytes gauge
walrus_max_allowed_bytes {} 8589934592

# HELP walrus_increases_total Total number of size increases
# TYPE walrus_increases_total counter
walrus_increases_total {} 5

# HELP walrus_decreases_total Total number of size decreases
# TYPE walrus_decreases_total counter
walrus_decreases_total {} 2

# HELP walrus_analysis_cycles_total Total analysis cycles run
# TYPE walrus_analysis_cycles_total counter
walrus_analysis_cycles_total {} 1440

# HELP walrus_forced_checkpoints Last observed forced checkpoint count
# TYPE walrus_forced_checkpoints gauge
walrus_forced_checkpoints {} 0

# HELP walrus_enabled Whether auto-sizing is enabled
# TYPE walrus_enabled gauge
walrus_enabled {} 1
```

## Integration
- Query via SQL: `SELECT walrus_metrics_prometheus()`
- Expose via HTTP using postgres_exporter or custom endpoint
- Scrape interval aligned with checkpoint_timeout

## State Tracking
- Use atomic counters for thread-safe metrics
- Reset on extension reload (or persist via shared memory)

## Dependencies
- Requires core extension (feature 01)

## Reference
- **Core implementation**: `src/worker.rs`, `src/stats.rs` - Reference for metrics to expose
- **Feature index**: `features/README.md`
