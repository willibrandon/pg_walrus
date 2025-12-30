# Background Worker Contract: pg_walrus

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite

## Overview

pg_walrus runs a single background worker process that monitors checkpoint activity and adjusts `max_wal_size` when necessary.

## Worker Registration

| Property | Value |
|----------|-------|
| Name | `pg_walrus` |
| Type | `pg_walrus` |
| Library | `pg_walrus` |
| Function | `walrus_worker_main` |
| Start Time | `BgWorkerStart_RecoveryFinished` |
| Restart Time | `checkpoint_timeout` (via extern C - see R8) |
| Flags | `BGWORKER_SHMEM_ACCESS` |

## Lifecycle

### Startup

1. PostgreSQL loads `pg_walrus` via `shared_preload_libraries`
2. `_PG_init()` called during postmaster startup
3. GUC parameters registered
4. Background worker registered (not yet started)
5. After recovery completes, worker process spawns
6. Worker attaches signal handlers (SIGHUP, SIGTERM)
7. Worker logs startup message
8. Worker enters main loop

### Main Loop

**Note**: `checkpoint_timeout` is accessed via extern C declaration (pgrx does not expose `pg_sys::CheckPointTimeout`). See research.md R8.

```text
WHILE wait_latch(checkpoint_timeout()) returns true:
    IF self-triggered SIGHUP flag is set:
        Clear flag and CONTINUE

    IF SIGHUP received:
        (GUC values auto-reloaded by PostgreSQL)

    IF walrus.enable is false:
        CONTINUE

    IF first_iteration:
        Store current checkpoint count as baseline
        Clear first_iteration flag
        CONTINUE

    Fetch current checkpoint statistics
    Calculate delta = current - previous
    Store current as previous

    IF delta >= walrus.threshold:
        Calculate new_size = max_wal_size_mb * (delta + 1)

        IF new_size > walrus.max:
            Log warning about cap
            new_size = walrus.max

        IF max_wal_size_mb == new_size:
            CONTINUE (already at cap)

        Log resize decision
        Execute ALTER SYSTEM SET max_wal_size = new_size
        Set self-trigger flag
        Send SIGHUP to postmaster
```

### Shutdown

1. SIGTERM received (PostgreSQL shutdown or `pg_terminate_backend()`)
2. `wait_latch()` returns `false`
3. Worker logs shutdown message
4. Worker exits gracefully

## Signal Handling

| Signal | Source | Action |
|--------|--------|--------|
| SIGHUP | Postmaster (config reload) | Re-read GUC values |
| SIGHUP | Self (after ALTER SYSTEM) | Skip next iteration (suppressed) |
| SIGTERM | Postmaster (shutdown) | Exit main loop gracefully |

## Process Visibility

```sql
-- View background worker in pg_stat_activity
SELECT pid, backend_type, application_name, state
FROM pg_stat_activity
WHERE backend_type = 'pg_walrus';

-- Expected output (when running):
--  pid  | backend_type | application_name | state
-- ------+--------------+------------------+-------
--  1234 | pg_walrus    | pg_walrus        | active
```

## Logging Contract

| Event | Log Level | Message Format |
|-------|-----------|----------------|
| Worker start | LOG | `pg_walrus worker started` |
| Baseline established | DEBUG1 | `pg_walrus: established baseline checkpoint count: %d` |
| Threshold met | LOG | `pg_walrus: detected %d forced checkpoints over %d seconds` |
| Resize decision | LOG | `pg_walrus: resizing max_wal_size from %d MB to %d MB` |
| Cap reached | WARNING | `pg_walrus: requested max_wal_size of %d MB exceeds maximum of %d MB; using maximum` |
| Already at cap | DEBUG1 | `pg_walrus: max_wal_size already at maximum (%d MB)` |
| Stats unavailable | WARNING | `pg_walrus: checkpoint statistics unavailable, skipping cycle` |
| Config reload | DEBUG1 | `pg_walrus: configuration reloaded` |
| Worker shutdown | LOG | `pg_walrus worker shutting down` |

## Error Handling

| Error Condition | Behavior |
|-----------------|----------|
| `pgstat_fetch_stat_checkpointer()` returns NULL | Log warning, skip cycle, retry next interval |
| `AlterSystemSetConfigFile()` fails | Log warning, skip resize, continue monitoring |
| Transaction fails | Abort transaction, log error, continue monitoring |
| Worker crash | PostgreSQL auto-restarts after `checkpoint_timeout` |

## Resource Usage

| Resource | Limit |
|----------|-------|
| Memory | < 1 MB |
| CPU | Negligible (wakes every checkpoint_timeout) |
| Connections | 0 (no database connection needed) |
| Shared Memory | Worker registration only |

## Constraints

1. **Primary Only**: Worker only runs on primary server (starts after recovery)
2. **Single Instance**: Exactly one worker per PostgreSQL cluster
3. **Non-Blocking**: Worker must never block PostgreSQL operations
4. **Idempotent**: Multiple identical resize attempts have no adverse effect
5. **Crash-Safe**: PostgreSQL automatically restarts crashed workers
