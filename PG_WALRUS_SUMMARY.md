# pg_walrus

## WAL + Rust = Walrus

**pg_walrus** is the Rust rewrite of pg_walsizer - a PostgreSQL extension that automatically monitors and adjusts `max_wal_size` to prevent performance-degrading forced checkpoints.

---

## Quick Overview

| Aspect | pg_walsizer (C) | pg_walrus (Rust) |
|--------|-----------------|------------------|
| Language | C | Rust (pgrx) |
| Lines of Code | ~300 | ~800 (with enhancements) |
| GUC Parameters | 3 | 15+ |
| SQL Functions | 0 | 5+ |
| Auto-Shrink | No | Yes |
| Monitoring | Logs only | Prometheus, NOTIFY, JSON API |
| Testing | Manual | Automated `#[pg_test]` |
| Memory Safety | Manual | Compile-time guarantees |

---

## Configuration (GUCs)

### Core Settings
```ini
walrus.enable = on              # Enable/disable auto-sizing
walrus.max = 8GB                # Maximum allowed max_wal_size
walrus.threshold = 2            # Forced checkpoints before resize
```

### Auto-Shrink (NEW)
```ini
walrus.shrink_enable = on       # Enable automatic shrinking
walrus.shrink_factor = 0.75     # Shrink to 75% when quiet
walrus.shrink_intervals = 5     # Quiet intervals before shrinking
walrus.min_size = 1GB           # Never shrink below this
```

### Rate Limiting (NEW)
```ini
walrus.cooldown_sec = 300       # Minimum seconds between changes
walrus.max_changes_per_hour = 4 # Maximum adjustments per hour
```

### Advanced (NEW)
```ini
walrus.algorithm = multiplicative  # or: additive, adaptive, percentile
walrus.dry_run = off               # Log recommendations without applying
walrus.history_retention_days = 30 # Days to keep history
```

---

## SQL Functions (NEW)

```sql
-- Get current status
SELECT walrus_status();
-- Returns: {"enabled":true,"current_max_wal_size_mb":2048,...}

-- View adjustment history
SELECT * FROM walrus_history() ORDER BY timestamp DESC LIMIT 10;

-- Get current recommendation
SELECT walrus_recommendation();
-- Returns: {"current_size_mb":2048,"recommended_size_mb":4096,"action":"increase"}

-- Trigger immediate analysis
SELECT walrus_analyze();

-- Get Prometheus metrics
SELECT walrus_metrics_prometheus();

-- Reset extension state
SELECT walrus_reset();
```

---

## Real-Time Notifications (NEW)

```sql
-- Subscribe to events
LISTEN walrus_events;

-- Receive notifications like:
-- {"event":"adjustment","action":"increase","old_size_mb":1024,"new_size_mb":2048}
```

---

## Installation

```bash
# Build
cargo pgrx package --pg-config /usr/bin/pg_config

# Install
sudo cp target/release/pg_walrus-pg17/usr/share/postgresql/17/extension/* \
       /usr/share/postgresql/17/extension/
sudo cp target/release/pg_walrus-pg17/usr/lib/postgresql/17/lib/* \
       /usr/lib/postgresql/17/lib/

# Enable
echo "shared_preload_libraries = 'pg_walrus'" >> postgresql.conf
pg_ctl restart
```

---

## PostgreSQL Version Support

- PostgreSQL 15
- PostgreSQL 16
- PostgreSQL 17
- PostgreSQL 18 (when released)

---

## Documentation

| Document | Description |
|----------|-------------|
| `CONVERSION_PROPOSAL.md` | Full technical design for C to Rust conversion |
| `ENHANCEMENTS_PROPOSAL.md` | New features and improvements |
| `README.md` | Original pg_walsizer documentation |

---

## Project Structure

```
pg_walrus/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Entry point, _PG_init
│   ├── worker.rs           # Background worker
│   ├── stats.rs            # Checkpoint statistics
│   ├── config.rs           # ALTER SYSTEM, GUCs
│   ├── history.rs          # Audit trail
│   └── version_compat.rs   # PG version handling
├── sql/
│   └── pg_walrus--1.0.0.sql
└── tests/
    └── integration_tests.rs
```

---

## Key Features

### From Original pg_walsizer
- Background worker monitoring checkpoint activity
- Automatic `max_wal_size` increases when forced checkpoints exceed threshold
- `ALTER SYSTEM` + `SIGHUP` for live configuration updates
- Configurable maximum cap to prevent runaway growth

### New in pg_walrus
- **Auto-Shrink**: Automatically reduce size when workload decreases
- **SQL Functions**: Query status, history, recommendations via SQL
- **History Table**: Full audit trail of all adjustments
- **Dry-Run Mode**: Test behavior without making changes
- **Rate Limiting**: Prevent thrashing on unstable workloads
- **NOTIFY Events**: Real-time notifications on adjustments
- **Prometheus Metrics**: Standard monitoring integration
- **Smart Algorithms**: Multiple sizing strategies (adaptive, percentile)
- **Memory Safety**: Rust's compile-time guarantees
- **Automated Testing**: Built-in `#[pg_test]` framework

---

## License

PostgreSQL License (same as original)

---

## Links

- **pg_walrus**: https://github.com/willibrandon/pg_walrus
- **Original pg_walsizer**: https://github.com/pgedge/pg_walsizer
- **pgrx Framework**: https://github.com/pgcentralfoundation/pgrx
