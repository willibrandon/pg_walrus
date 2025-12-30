---
description: Execute the implementation planning workflow using the plan template to generate design artifacts.
handoffs: 
  - label: Create Tasks
    agent: speckit.tasks
    prompt: Break the plan into tasks
    send: true
  - label: Create Checklist
    agent: speckit.checklist
    prompt: Create a checklist for the following domain...
---

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

## Outline

1. **Setup**: Run `.specify/scripts/bash/setup-plan.sh --json` from repo root and parse JSON for FEATURE_SPEC, IMPL_PLAN, SPECS_DIR, BRANCH. For single quotes in args like "I'm Groot", use escape syntax: e.g 'I'\''m Groot' (or double-quote if possible: "I'm Groot").

2. **Load context**: Read FEATURE_SPEC and `.specify/memory/constitution.md`. Load IMPL_PLAN template (already copied).

3. **Execute plan workflow**: Follow the structure in IMPL_PLAN template to:
   - Fill Technical Context (mark unknowns as "NEEDS CLARIFICATION")
   - Fill Constitution Check section from constitution
   - Evaluate gates (ERROR if violations unjustified)
   - Phase 0: Generate research.md (resolve all NEEDS CLARIFICATION)
   - Phase 1: Generate data-model.md, contracts/, quickstart.md
   - Phase 1: Update agent context by running the agent script
   - Re-evaluate Constitution Check post-design

4. **Stop and report**: Command ends after Phase 2 planning. Report branch, IMPL_PLAN path, and generated artifacts.

## Phases

### Phase 0: Outline & Research

1. **Extract unknowns from Technical Context** above:
   - For each NEEDS CLARIFICATION → research task
   - For each dependency → best practices task
   - For each integration → patterns task

2. **Generate and dispatch research agents**:

   ```text
   For each unknown in Technical Context:
     Task: "Research {unknown} for {feature context}"
   For each technology choice:
     Task: "Find best practices for {tech} in {domain}"
   ```

3. **Consolidate findings** in `research.md` using format:
   - Decision: [what was chosen]
   - Rationale: [why chosen]
   - Alternatives considered: [what else evaluated]

**Output**: research.md with all NEEDS CLARIFICATION resolved

### Phase 1: Design & Contracts

**Prerequisites:** `research.md` complete

1. **Extract entities from feature spec** → `data-model.md`:
   - Entity name, fields, relationships
   - Validation rules from requirements
   - State transitions if applicable

2. **Generate API contracts** from functional requirements:
   - For each user action → endpoint
   - Use standard REST/GraphQL patterns
   - Output OpenAPI/GraphQL schema to `/contracts/`

3. **Agent context update**:
   - Run `.specify/scripts/bash/update-agent-context.sh claude`
   - These scripts detect which AI agent is in use
   - Update the appropriate agent-specific context file
   - Add only new technology from current plan
   - Preserve manual additions between markers

**Output**: data-model.md, /contracts/*, quickstart.md, agent-specific file

## Key rules

- Use absolute paths
- ERROR on gate failures or unresolved clarifications

## MANDATORY: pgrx Background Worker Testing Requirement

Extensions with background workers MUST include a `pg_test` module at crate root with `postgresql_conf_options()`:

```rust
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='extension_name'"]
    }
}
```

Without this module, background worker tests WILL FAIL. pgrx-tests calls this function to configure PostgreSQL BEFORE startup.

## No False Impossibility Claims (Constitution XVI)

Claiming that tests or implementations are "impossible" is PROHIBITED. You have full source code access.

**You have NO excuse for claiming impossibility:**
- `/Users/brandon/src/pgrx/` - Full pgrx source code with examples and tests
- `/Users/brandon/src/postgres/` - Full PostgreSQL source code with implementation details
- `pg_settings` system catalog with `min_val`, `max_val`, `vartype`, `context` columns

**The test/implementation is NEVER impossible. The approach is wrong. Fix the approach.**

## pgrx Reference

**Local pgrx Repository**: `/Users/brandon/src/pgrx/`
- Consult this repository for pgrx API patterns, examples, and best practices when planning pgrx extension development
- Key directories:
  - `pgrx/` - Core framework code
  - `pgrx-examples/` - Example extensions demonstrating patterns
  - `pgrx-macros/` - Procedural macro implementations

## PostgreSQL Reference

**Local PostgreSQL Source**: `/Users/brandon/src/postgres/`
- Consult this repository for PostgreSQL internal APIs, struct definitions, and implementation details
- Key directories for extension development:
  - `src/backend/postmaster/checkpointer.c` - Checkpointer process implementation
  - `src/backend/postmaster/bgworker.c` - Background worker infrastructure
  - `src/backend/utils/misc/guc.c` - GUC (Grand Unified Configuration) system
  - `src/include/pgstat.h` - Statistics collector definitions
  - `src/backend/commands/variable.c` - ALTER SYSTEM implementation
