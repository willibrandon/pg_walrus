# pg_walrus Feature Documents

Feature documents for `/speckit.specify` workflow. Each file is optimized to be passed as `$ARGUMENTS`.

## Reference Implementation

The original C implementation (`pg_walsizer/`) serves as the reference for the Rust rewrite. Use it during all speckit phases:

| File | Purpose |
|------|---------|
| `pg_walsizer/walsizer.c` | Core logic, background worker, GUC registration |
| `pg_walsizer/walsizer.h` | Header with exports |
| `pg_walsizer/README.md` | Original documentation |
| `CONVERSION_PROPOSAL.md` | C-to-Rust API mappings and design decisions |
| `ENHANCEMENTS_PROPOSAL.md` | New features beyond the original |

## Usage

```bash
/speckit.specify features/01-core-extension.md
/speckit.specify features/02-auto-shrink.md
# etc.
```

## Feature Index

### Tier 1: Essential (v1.0)
| # | Feature | Description | Dependencies |
|---|---------|-------------|--------------|
| 01 | Core Extension | pgrx rewrite of pg_walsizer | None |
| 02 | Auto-Shrink | Automatic size decrease | 01 |
| 03 | SQL Functions | Observability via SQL | 01, 04 |
| 04 | History Table | Audit trail | 01 |
| 05 | Dry-Run Mode | Test without changes | 01 |
| 06 | Rate Limiting | Prevent thrashing | 01 |
| 07 | NOTIFY Events | Real-time notifications | 01 |

### Tier 2: Professional (v1.1)
| # | Feature | Description | Dependencies |
|---|---------|-------------|--------------|
| 08 | Prometheus Metrics | Standard monitoring | 01 |
| 09 | Smart Algorithms | Multiple sizing options | 01 |

### Tier 3: Enterprise (v2.0)
| # | Feature | Description | Dependencies |
|---|---------|-------------|--------------|
| 10 | Time Profiles | Schedule-aware config | 01 |
| 11 | WAL Rate Tracking | Predictive sizing | 01, 09 |
| 12 | Shared Memory | Persistent state | 01 |

## Recommended Implementation Order

1. **01-core-extension** - Foundation (must be first)
2. **04-history-table** - Needed by SQL functions
3. **05-dry-run-mode** - Simple, enables safe testing
4. **06-rate-limiting** - Stability improvement
5. **02-auto-shrink** - Core enhancement
6. **03-sql-functions** - Observability
7. **07-notify-events** - Real-time awareness
8. **08-prometheus-metrics** - Monitoring integration
9. **09-smart-algorithms** - Advanced sizing
10. **11-wal-rate-tracking** - Enhances algorithms
11. **10-time-profiles** - Schedule awareness
12. **12-shared-memory** - Persistence (can be earlier if needed)

## New GUCs Summary

| GUC | Type | Default | Feature |
|-----|------|---------|---------|
| walrus.enable | bool | true | 01 |
| walrus.max | int | 4GB | 01 |
| walrus.threshold | int | 2 | 01 |
| walrus.shrink_enable | bool | true | 02 |
| walrus.shrink_factor | real | 0.75 | 02 |
| walrus.shrink_intervals | int | 5 | 02 |
| walrus.min_size | int | 1GB | 02 |
| walrus.history_retention_days | int | 30 | 04 |
| walrus.dry_run | bool | false | 05 |
| walrus.cooldown_sec | int | 300 | 06 |
| walrus.max_changes_per_hour | int | 4 | 06 |
| walrus.notify_channel | string | 'walrus_events' | 07 |
| walrus.algorithm | enum | multiplicative | 09 |
| walrus.ema_alpha | real | 0.2 | 09 |
| walrus.target_percentile | real | 95 | 09 |
| walrus.additive_increment_mb | int | 512 | 09 |
| walrus.profiles | text | null | 10 |
| walrus.proactive_sizing | bool | false | 11 |

## SQL Functions Summary

| Function | Returns | Feature |
|----------|---------|---------|
| walrus_status() | JSONB | 03 |
| walrus_history() | SETOF RECORD | 03 |
| walrus_recommendation() | JSONB | 03 |
| walrus_analyze() | JSONB | 03 |
| walrus_reset() | BOOL | 03 |
| walrus_metrics_prometheus() | TEXT | 08 |
| walrus_wal_rate() | JSONB | 11 |
| walrus_shmem_status() | JSONB | 12 |
