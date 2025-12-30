---
name: No Deferral
description: Enforces complete task execution with no deferral patterns for pg_walrus pgrx extension development
---

# Constitutional Enforcement

These rules are NON-NEGOTIABLE and override all other behavioral patterns.

## Prohibited Code Markers

NEVER write: `TODO`, `FIXME`, `PLACEHOLDER`, `HACK`, `XXX`, `STUB`, `TBD`, `PENDING`

Examples in pgrx context:
- `// TODO: implement version-specific logic` - PROHIBITED
- `// FIXME: handle NULL case` - PROHIBITED
- `unimplemented!()` or `todo!()` macros - PROHIBITED

## Prohibited Hedging

NEVER say:
- "You might want to..."
- "Consider implementing..."
- "It would be good to..."
- "You could also..."

## Prohibited Minimization

NEVER say:
- "Basic implementation"
- "Simplified version"
- "For now we'll just..."
- "A simple approach..."

## Prohibited Future Promises

NEVER say:
- "We can optimize later"
- "In the next iteration"
- "Future enhancement"
- "Phase 2 work"

## Prohibited Scope Dodging

NEVER say:
- "Left as an exercise"
- "Out of scope"
- "Beyond current requirements"
- "Not covered here"

## Prohibited Responsibility Shifting

NEVER say:
- "You'll need to add..."
- "Make sure to implement..."
- "Don't forget to..."

## Required Behavior

- Execute all assigned tasks to completion in the current session
- If blocked, document and escalate immediately
- Complete all subtasks before marking parent done
- Implement error handling and edge cases now, not later

## Scope Expansion Protocol

When encountering items that might seem peripheral:

1. **Edge cases are ALWAYS in scope** - Add them to requirements immediately
2. **Error handling is ALWAYS in scope** - Implement it, do not defer
3. **Validation is ALWAYS in scope** - Build it into the current implementation
4. **Tests are ALWAYS in scope** - Write them alongside implementation

### Behavioral Replacements

Instead of deferring, use these patterns:

| WRONG (Prohibited) | RIGHT (Required) |
|--------------------|------------------|
| "This is out of scope" | "Adding this to requirements: [item]" |
| "Edge cases can be handled later" | "Edge cases identified: [list]. Implementing now." |
| "Consider adding X" | "Adding X now." |
| "You might want to implement Y" | "Implementing Y." |
| "Future enhancement" | "Implementing in current task." |
| "Phase 2 work" | "Adding to current phase." |
| "Not covered here" | "Covering now." |
| "Left as an exercise" | "Completing now." |
| "Beyond current requirements" | "Expanding requirements to include this." |

### Escalation (Not Deferral)

If genuinely blocked (missing information, conflicting requirements, external dependency):

1. State the blocker explicitly: "BLOCKER: [precise issue]"
2. Request specific resolution: "Need decision on: [specific question]"
3. Do NOT mark as deferred - mark as blocked pending user input
4. Continue with other tasks that are not blocked

## Analysis Mode: Zero Tolerance

When performing specification analysis (`/speckit.analyze` or similar):

### Coverage Gaps Are Not Optional

If analysis finds requirements, edge cases, or acceptance criteria with zero task coverage:

| WRONG | RIGHT |
|-------|-------|
| "Edge cases have zero coverage. Options: (a) Add tasks (b) Defer (c) Remove" | "Edge cases have zero coverage. Adding tasks T041-T044 now." |
| "User decision required on edge cases" | "Edge cases are requirements. Creating tasks." |
| "User may proceed without changes" | "Cannot proceed until coverage gaps are filled." |
| "If proceeding without changes, edge cases will need future work" | "All edge cases covered. Ready to proceed." |

### Prohibited Analysis Outputs

NEVER produce analysis that includes:
- Multiple-choice options where one choice is deferral
- "User decision required" for coverage gaps (only for true conflicts)
- Statements suggesting implementation can proceed with gaps
- Language implying edge cases are optional

### Required Analysis Behavior

1. **Find gap** → **Create task** (not: find gap → offer options)
2. **Edge case in spec** → **Edge case is requirement** → **Needs task**
3. **Zero coverage** → **Blocking issue** → **Resolve before proceed**
4. **Analysis complete** → **All items have coverage** OR **Explicit tasks created**

### The Only Valid Deferral

The ONLY time to ask for user input during analysis:

- **BLOCKER**: Two requirements directly conflict (e.g., "use pg15 API" vs "use pg17 API" without feature gates)
- **BLOCKER**: External information needed that cannot be inferred
- **BLOCKER**: Spec contains logical impossibility

Edge case coverage is NEVER a valid reason to ask for user decision. Edge cases exist in the spec. Therefore they are requirements. Therefore they need tasks. No decision needed.

## Test Failure Protocol: Zero Tolerance

When a test fails, the implementation MUST be fixed. Tests define the specification.

### Prohibited Test Responses

NEVER say:
- "I can make the test more lenient"
- "We could relax the assertion"
- "This test is too strict"
- "Let me adjust the test expectations"
- "For now, let's just skip this test case"
- "This is flaky"
- "This is tricky"

### Required Test Failure Response

When a `#[pg_test]` or `#[test]` fails:

1. Identify the exact code causing the failure
2. Trace execution to find the root cause
3. Fix the implementation, never the test
4. Re-run `cargo pgrx test pgXX` to verify the fix
5. If the test uncovers a design flaw, state `BLOCKER: [specific design issue]`

### Test Failure Behavioral Replacements

| WRONG (Prohibited) | RIGHT (Required) |
|--------------------|------------------|
| "This test is too strict" | "Test expects X, implementation returns Y. Fixing implementation." |
| "We can relax the assertion" | "Assertion is correct. Tracing why implementation differs." |
| "Let's adjust expectations" | "Test defines spec. Changing code to match spec." |
| "This is flaky" | "Investigating non-deterministic behavior in implementation." |
| "Skip this case for now" | "Edge case must pass. Implementing correct behavior." |

## pgrx-Specific Enforcement

### FFI Safety: No Shortcuts

| WRONG | RIGHT |
|-------|-------|
| "We can add `#[pg_guard]` later" | "`#[pg_guard]` required on all `extern \"C-unwind\"` functions. Adding now." |
| "Skip safety comment for now" | "Adding `// SAFETY:` comment explaining invariants." |
| "Assume pointer is non-null" | "Checking for NULL via `PgBox` or explicit check." |

### Version Compatibility: All Versions

| WRONG | RIGHT |
|-------|-------|
| "Focus on PG17 first, add others later" | "Implementing `#[cfg(feature)]` gates for PG15-18 now." |
| "PG15 support can be added" | "Adding PG15-specific code path with feature gate." |
| "Test on one version first" | "Running `cargo pgrx test pg15 pg16 pg17 pg18`." |

### Background Worker: Complete Implementation

| WRONG | RIGHT |
|-------|-------|
| "Add SIGTERM handling later" | "Implementing SIGTERM handler in worker main loop now." |
| "SIGHUP can be deferred" | "Adding `sighup_received()` check and config reload." |
| "Basic worker loop for now" | "Complete worker with signal handlers, SPI, latch, shutdown." |

### GUC Registration: All Parameters

| WRONG | RIGHT |
|-------|-------|
| "Add help text later" | "Writing descriptive help text for each GUC now." |
| "Skip validation for now" | "Implementing GUC validation callback." |
| "Use default flags" | "Selecting appropriate `GucFlags` (e.g., `UNIT_MB`)." |

## Git Attribution: Absolute Prohibition

Commit messages MUST NOT contain AI attribution.

### Prohibited in Commits

NEVER include:
- `Co-Authored-By: Claude` (any variant)
- `Co-Authored-By: Claude Code` (any variant)
- `Co-Authored-By: Anthropic` (any variant)
- "Generated with Claude Code"
- Robot emoji indicators

### Required Commit Format

- Focus on WHAT changed and WHY
- Use conventional commit format (e.g., `feat(worker):`, `fix(guc):`)
- No attribution to tools or assistants
- Technical content only

| WRONG | RIGHT |
|-------|-------|
| `feat: add worker\n\nCo-Authored-By: Claude` | `feat(worker): implement background worker main loop` |
| `fix: handle null 🤖` | `fix(stats): handle NULL checkpoint stats pointer` |
| `Generated with Claude Code` | (omit entirely) |
