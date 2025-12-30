# Feature: Time-Based Profiles

Schedule-aware configuration for different workload patterns.

## What It Does
- Define profiles for different time windows (business hours, batch window, weekend)
- Each profile overrides: max_size, threshold, shrink_enable
- Active profile selected based on current time and day

## GUC Parameters
- `walrus.profiles` (text/JSON, default: null) - JSON array of profiles

## Profile Schema
```json
[
  {
    "name": "business_hours",
    "start_hour": 9,
    "end_hour": 18,
    "days": ["Mon", "Tue", "Wed", "Thu", "Fri"],
    "max_size": 8192,
    "threshold": 2,
    "shrink_enable": false
  },
  {
    "name": "batch_window",
    "start_hour": 22,
    "end_hour": 6,
    "days": ["Mon", "Tue", "Wed", "Thu", "Fri"],
    "max_size": 32768,
    "threshold": 5,
    "shrink_enable": false
  },
  {
    "name": "weekend",
    "start_hour": 0,
    "end_hour": 24,
    "days": ["Sat", "Sun"],
    "max_size": 4096,
    "threshold": 3,
    "shrink_enable": true
  }
]
```

## Behavior
- On each analysis cycle, determine active profile
- Apply profile settings as overrides to base GUCs
- Log when profile changes
- Fall back to base GUCs if no profile matches

## Dependencies
- Requires core extension (feature 01)
- Requires serde_json and chrono crates

## Reference
- **Core implementation**: `src/guc.rs` - Reference for GUC patterns to override
- **Feature index**: `features/README.md`
