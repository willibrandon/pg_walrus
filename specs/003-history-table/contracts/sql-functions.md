# SQL Function Contracts: History Table

**Feature**: 003-history-table
**Date**: 2025-12-30

## Functions

### walrus.cleanup_history()

Deletes history records older than the configured retention period.

**Signature**:
```sql
FUNCTION walrus.cleanup_history() RETURNS BIGINT
```

**Parameters**: None

**Returns**: Number of records deleted

**Behavior**:
1. Reads `walrus.history_retention_days` GUC value
2. Deletes all records from `walrus.history` where `timestamp < now() - interval 'N days'`
3. Returns count of deleted records

**Example Usage**:
```sql
-- Manual cleanup
SELECT walrus.cleanup_history();
-- Returns: 42 (number of deleted records)

-- Schedule with pg_cron (optional, worker already calls this)
SELECT cron.schedule('walrus-cleanup', '0 * * * *', 'SELECT walrus.cleanup_history()');
```

**Error Handling**:
- Returns 0 if no records to delete
- Raises ERROR if table does not exist (extension not installed properly)

---

## GUC Parameters

### walrus.history_retention_days

Controls how long history records are retained before automatic cleanup.

**Declaration**:
```sql
-- View current value
SHOW walrus.history_retention_days;

-- Change value (requires reload)
ALTER SYSTEM SET walrus.history_retention_days = 30;
SELECT pg_reload_conf();
```

**Properties**:
| Property | Value |
|----------|-------|
| Type | INTEGER |
| Default | 7 |
| Minimum | 0 |
| Maximum | 3650 |
| Context | sighup |
| Unit | days |

**Special Values**:
- `0`: All records deleted on each cleanup (effectively disables history)
- `3650`: 10-year retention (maximum)

---

## Table Schema Contract

### walrus.history

**Guaranteed Columns** (stable API):

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| id | BIGINT | NO | Unique identifier |
| timestamp | TIMESTAMPTZ | NO | When decision was made |
| action | TEXT | NO | 'increase', 'decrease', or 'capped' |
| old_size_mb | INTEGER | NO | Previous max_wal_size |
| new_size_mb | INTEGER | NO | New max_wal_size |
| forced_checkpoints | BIGINT | NO | Checkpoint count at decision |
| checkpoint_timeout_sec | INTEGER | NO | Timeout value at decision |
| reason | TEXT | YES | Human-readable explanation |
| metadata | JSONB | YES | Algorithm details |

**Query Examples**:

```sql
-- Most recent 10 decisions
SELECT * FROM walrus.history ORDER BY timestamp DESC LIMIT 10;

-- All increases in last 24 hours
SELECT * FROM walrus.history
WHERE action = 'increase'
  AND timestamp > now() - interval '24 hours';

-- Summary by action type
SELECT action, count(*), avg(new_size_mb - old_size_mb) as avg_change
FROM walrus.history
GROUP BY action;

-- Export for audit
COPY (
    SELECT * FROM walrus.history
    WHERE timestamp >= '2025-01-01'
    ORDER BY timestamp
) TO '/tmp/walrus-audit.csv' WITH CSV HEADER;
```

---

## Internal Rust Functions (Not SQL-Callable)

These functions are internal implementation details, not part of the SQL API:

### insert_history_record

**Purpose**: Called by background worker after each sizing decision.

**Parameters**:
- `action: &str` - 'increase', 'decrease', or 'capped'
- `old_size: i32` - Previous max_wal_size in MB
- `new_size: i32` - New max_wal_size in MB
- `forced_checkpoints: i64` - Checkpoint count
- `timeout_sec: i32` - checkpoint_timeout in seconds
- `reason: Option<&str>` - Optional explanation
- `metadata: Option<serde_json::Value>` - Optional algorithm details

**Returns**: `Result<(), spi::Error>`

**Transaction**: Called within `BackgroundWorker::transaction()` context.

### cleanup_old_history

**Purpose**: Called by background worker after each monitoring cycle.

**Parameters**: None

**Returns**: `Result<i64, spi::Error>` - Count of deleted records

**Transaction**: Called within `BackgroundWorker::transaction()` context.

**Note**: This is the internal implementation called by the worker. The SQL-callable `walrus.cleanup_history()` is a wrapper that calls this function.
