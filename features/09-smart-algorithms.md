# Feature: Smart Sizing Algorithms

Multiple algorithm options for calculating optimal `max_wal_size`.

## Algorithms

### Multiplicative (Default, Original)
`new_size = current * (forced_checkpoints + 1)`
- Simple, aggressive growth
- May overshoot on spike workloads

### Additive
`new_size = current + (forced_checkpoints * increment)`
- Linear growth
- `walrus.additive_increment_mb` (int, default: 512)

### Adaptive (EMA)
- Tracks exponential moving average of WAL generation rate
- Calculates: `ema_rate * checkpoint_timeout * 1.5`
- `walrus.ema_alpha` (real, default: 0.2) - Smoothing factor

### Percentile
- Keeps history of WAL generation samples
- Targets specific percentile of historical usage
- `walrus.target_percentile` (real, default: 95)

## GUC Parameters
- `walrus.algorithm` (enum, default: multiplicative) - Algorithm selection
  - Values: multiplicative, additive, adaptive, percentile

## Implementation
```rust
#[derive(PostgresGucEnum)]
pub enum SizingAlgorithm {
    Multiplicative,
    Additive,
    Adaptive,
    Percentile,
}
```

## Dependencies
- Requires core extension (feature 01)
- Adaptive/Percentile require sample storage (in-memory or history table)

## Reference
- **Core implementation**: `src/worker.rs` - Reference for original multiplicative algorithm (calculate_new_size)
- **Feature index**: `features/README.md`
