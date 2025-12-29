# Feature: Shared Memory State

Persistent state across worker restarts and cross-process visibility.

## What It Does
- Stores extension state in PostgreSQL shared memory
- Survives background worker restarts
- Accessible from any backend (for SQL functions)
- Enables accurate metrics without database queries

## Shared Memory Structure
```rust
#[derive(Copy, Clone)]
struct WalrusShmem {
    magic: u32,                    // Validation marker
    total_increases: u64,
    total_decreases: u64,
    last_adjustment_time: i64,     // Unix timestamp
    last_forced_checkpoints: i64,
    current_recommendation: i32,
    worker_pid: i32,
    quiet_intervals: u32,
    rate_samples: [f64; 100],      // For WAL rate tracking
    rate_sample_count: u32,
}
```

## pgrx Integration
```rust
unsafe impl PGRXSharedMemory for WalrusShmem {}
static WALRUS_SHMEM: PgSharedMem<WalrusShmem> = PgSharedMem::new();

#[pg_guard]
pub extern "C" fn _PG_init() {
    pg_shmem_init!(WALRUS_SHMEM);
}
```

## Benefits
- Metrics counters persist across restarts
- SQL functions read directly from shmem (fast)
- No database queries needed for status
- WAL rate samples survive worker restart

## SQL Function
`walrus_shmem_status() -> JSONB` - Direct read from shared memory

## Dependencies
- Requires core extension (feature 01)
- Enhances all other features with persistent state

## Reference
- **pg_walsizer source**: `pg_walsizer/walsizer.c` - Reference for state to persist
- **Enhancements design**: `ENHANCEMENTS_PROPOSAL.md` - Section 4.4 Shared Memory
- **Feature index**: `features/README.md`
