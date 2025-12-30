# Feature: History Table

Persistent audit trail of all sizing decisions for analysis and compliance.

## Schema

```sql
CREATE TABLE walrus.history (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL,  -- 'increase', 'decrease', 'no_change', 'capped', 'dry_run'
    old_size_mb INTEGER NOT NULL,
    new_size_mb INTEGER NOT NULL,
    forced_checkpoints BIGINT NOT NULL,
    checkpoint_timeout_sec INTEGER NOT NULL,
    reason TEXT,
    metadata JSONB
);

CREATE INDEX ON walrus.history (timestamp);
```

## Automatic Cleanup
- `walrus.history_retention_days` (int, default: 30) - Days to keep history
- Cleanup function: `walrus.cleanup_history()` deletes old records
- Optionally integrate with pg_cron or call from background worker

## Behavior
- Every sizing decision (increase, decrease, capped, dry_run) logged
- Metadata JSONB stores algorithm details, rates, etc.
- Used by `walrus_history()` SQL function

## Dependencies
- Requires core extension (feature 01)
- Schema created via `extension_sql!` macro

## Reference
- **Core implementation**: `src/worker.rs` - Reference for adjustment events to log
- **Feature index**: `features/README.md`
