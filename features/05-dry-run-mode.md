# Feature: Dry-Run Mode

Test extension behavior without making actual configuration changes.

## What It Does
- When enabled, logs what changes WOULD be made
- Records decisions to history table with action='dry_run'
- Does NOT execute ALTER SYSTEM or SIGHUP
- Allows safe validation in production

## GUC Parameters
- `walrus.dry_run` (bool, default: false) - Enable dry-run mode

## Log Output
```
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 1024 MB to 2048 MB (threshold exceeded)
```

## History Entry
```json
{
  "action": "dry_run",
  "old_size_mb": 1024,
  "new_size_mb": 2048,
  "metadata": {"dry_run": true, "would_apply": "increase"}
}
```

## Use Cases
- Validate behavior before enabling in production
- Test threshold/algorithm tuning
- Audit what decisions would be made

## Dependencies
- Requires core extension (feature 01)
- Integrates with history table (feature 04) if available

## Reference
- **pg_walsizer source**: `pg_walsizer/walsizer.c` - Reference for ALTER SYSTEM flow to bypass
- **Enhancements design**: `ENHANCEMENTS_PROPOSAL.md` - Section 2.4 Dry-Run Mode
- **Feature index**: `features/README.md`
