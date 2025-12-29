# Feature: Auto-Shrink

Automatically decrease `max_wal_size` when workload decreases, preventing permanent storage growth.

## What It Does
- Tracks consecutive "quiet intervals" (intervals with forced checkpoints below threshold)
- After N quiet intervals, shrinks `max_wal_size` by configurable factor (e.g., 75%)
- Never shrinks below configurable minimum floor
- Resets quiet interval counter when forced checkpoints occur

## GUC Parameters
- `walrus.shrink_enable` (bool, default: true) - Enable automatic shrinking
- `walrus.shrink_factor` (real, default: 0.75) - Multiply by this when shrinking
- `walrus.shrink_intervals` (int, default: 5) - Quiet intervals before shrinking
- `walrus.min_size` (int, default: 1GB) - Never shrink below this

## Behavior
- Shrink decision runs after normal threshold check
- If quiet_intervals >= shrink_intervals AND current_size > min_size: shrink
- New size = current_size * shrink_factor (rounded up)
- Logs shrink events same as increase events

## Dependencies
- Requires core extension (feature 01)

## Reference
- Feature index: `features/README.md`
