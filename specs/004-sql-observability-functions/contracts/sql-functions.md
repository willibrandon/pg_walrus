# SQL Function Contracts

**Branch**: `004-sql-observability-functions` | **Date**: 2025-12-30

## Function Signatures

### walrus.status()

```sql
CREATE FUNCTION walrus.status() RETURNS JSONB
```

**Returns JSONB with structure**:
```json
{
  "enabled": true,
  "current_max_wal_size_mb": 1024,
  "configured_maximum_mb": 4096,
  "threshold": 2,
  "checkpoint_timeout_sec": 300,
  "shrink_enabled": true,
  "shrink_factor": 0.75,
  "shrink_intervals": 5,
  "min_size_mb": 1024,
  "worker_running": true,
  "last_check_time": "2025-12-30T10:15:30.000000+00:00",
  "last_adjustment_time": "2025-12-30T09:45:00.000000+00:00",
  "total_adjustments": 5,
  "quiet_intervals": 2,
  "at_ceiling": false
}
```

**Authorization**: Public (any user)

**Behavior**:
- Returns current extension state snapshot
- Time fields are null if worker hasn't completed first cycle
- `at_ceiling` is true when `current_max_wal_size_mb >= configured_maximum_mb`
- `worker_running` checks pg_stat_activity for backend_type = 'pg_walrus'

---

### walrus.history()

```sql
CREATE FUNCTION walrus.history() RETURNS SETOF RECORD
    (timestamp TIMESTAMPTZ, action TEXT, old_size_mb INTEGER,
     new_size_mb INTEGER, forced_checkpoints BIGINT, reason TEXT)
```

**Returns set of records with columns**:
| Column | Type | Description |
|--------|------|-------------|
| timestamp | TIMESTAMPTZ | When the sizing decision was made |
| action | TEXT | "increase", "decrease", or "capped" |
| old_size_mb | INTEGER | max_wal_size before change |
| new_size_mb | INTEGER | max_wal_size after change |
| forced_checkpoints | BIGINT | Checkpoint count at decision time |
| reason | TEXT | Human-readable explanation |

**Authorization**: Public (any user)

**Behavior**:
- Returns all rows from walrus.history table
- Returns empty set if no adjustments have occurred
- Ordered by timestamp ASC by default (user can override with ORDER BY)
- If history table was dropped, returns error

**Error Handling**:
- Table dropped: Returns SQL error (not crash)

---

### walrus.recommendation()

```sql
CREATE FUNCTION walrus.recommendation() RETURNS JSONB
```

**Returns JSONB with structure**:
```json
{
  "current_size_mb": 1024,
  "recommended_size_mb": 2048,
  "action": "increase",
  "reason": "3 forced checkpoints detected in last interval",
  "confidence": 85
}
```

**Action values**:
- `"increase"`: Checkpoint activity warrants size increase
- `"decrease"`: Sustained low activity warrants shrink
- `"none"`: Current size is optimal
- `"error"`: Cannot compute (stats unavailable)

**Authorization**: Public (any user)

**Behavior**:
- Computes recommendation without applying changes
- Uses same algorithm as background worker
- Reads shared memory state (prev_requested) for delta calculation
- Returns `action: "error"` if checkpoint stats unavailable

**Confidence Calculation**:
- Base: 50
- +20 if checkpoint count > 10
- +15 if quiet_intervals > 0
- +15 if prev_requested > 0
- Returns 0 with `action: "error"` if stats unavailable

---

### walrus.analyze(apply boolean DEFAULT false)

```sql
CREATE FUNCTION walrus.analyze(apply boolean DEFAULT false) RETURNS JSONB
```

**Returns JSONB with structure**:
```json
{
  "analyzed": true,
  "recommendation": {
    "current_size_mb": 1024,
    "recommended_size_mb": 2048,
    "action": "increase",
    "reason": "3 forced checkpoints detected",
    "confidence": 85
  },
  "applied": false
}
```

**Authorization**:
- `apply = false`: Any user
- `apply = true`: Superuser only

**Behavior**:
- When `apply = false`: Returns analysis without changing anything
- When `apply = true` AND superuser AND action != "none": Executes ALTER SYSTEM
- `applied` is true ONLY when `apply` param is true AND change was executed
- If walrus.enable = false: Returns `{ "analyzed": false, "reason": "extension is disabled" }`
- Analysis runs in SQL session context (independent of background worker)

**Error Cases**:
- Non-superuser with `apply = true`: ERROR "permission denied"
- Stats unavailable: Returns `analyzed: true` with `action: "error"` in recommendation

---

### walrus.reset()

```sql
CREATE FUNCTION walrus.reset() RETURNS BOOLEAN
```

**Returns**: `true` on success, `false` on failure

**Authorization**: Superuser only

**Behavior**:
- Clears all rows from walrus.history table
- Resets shared memory counters to zero:
  - quiet_intervals = 0
  - total_adjustments = 0
  - prev_requested = 0
  - last_check_time = 0
  - last_adjustment_time = 0
- Worker sees reset state on next cycle (no signaling needed)

**Error Cases**:
- Non-superuser: ERROR "permission denied"
- History table dropped: WARNING logged, returns true (reset succeeds for shmem)

## Error Response Patterns

### Permission Denied
```sql
SELECT walrus.analyze(apply := true);
-- ERROR:  permission denied: walrus.analyze(apply := true) requires superuser

SELECT walrus.reset();
-- ERROR:  permission denied: walrus.reset() requires superuser
```

### Stats Unavailable
```sql
SELECT walrus.recommendation();
-- Returns: {"action": "error", "reason": "checkpoint statistics unavailable", "confidence": 0, ...}

SELECT walrus.analyze();
-- Returns: {"analyzed": true, "recommendation": {"action": "error", ...}, "applied": false}
```

### Extension Disabled
```sql
-- With walrus.enable = false
SELECT walrus.analyze();
-- Returns: {"analyzed": false, "reason": "extension is disabled"}
```

### History Table Issues
```sql
-- If walrus.history table dropped
SELECT * FROM walrus.history();
-- ERROR:  relation "walrus.history" does not exist

SELECT walrus.reset();
-- WARNING:  pg_walrus: history table does not exist
-- Returns: true (shmem reset succeeded)
```
