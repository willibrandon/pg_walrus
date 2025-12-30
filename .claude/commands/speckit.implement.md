---
description: Execute the implementation plan by processing and executing all tasks defined in tasks.md
---

## Constitutional Compliance (Constitution VI)

This command is bound by the No Task Deferral principle. The following are PROHIBITED:

**In generated code:**
- Code markers: `TODO`, `FIXME`, `PLACEHOLDER`, `HACK`, `XXX`, `STUB`, `TBD`, `PENDING`
- Incomplete implementations: Functions that throw "not implemented" without full logic
- Partial implementations: Missing error handling, edge cases, or validation

**In comments and output:**
- Hedging: "you might want to...", "consider adding...", "it would be good to..."
- Future promises: "we can optimize later", "phase 2 work", "future enhancement"
- Responsibility shifting: "you'll need to add...", "don't forget to..."

**When blocked:**
- State `BLOCKER: [specific issue]` and request a specific decision
- Do NOT use "suggest next steps" or defer to future work
- Do NOT halt and leave tasks incomplete without explicit BLOCKER escalation

**Completion requirement:**
- Each task MUST be fully implemented before marking complete
- Edge cases listed in spec.md MUST be handled, not deferred
- Error handling MUST be implemented, not stubbed

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

## Outline

1. Run `.specify/scripts/bash/check-prerequisites.sh --json --require-tasks --include-tasks` from repo root and parse FEATURE_DIR and AVAILABLE_DOCS list. All paths must be absolute. For single quotes in args like "I'm Groot", use escape syntax: e.g 'I'\''m Groot' (or double-quote if possible: "I'm Groot").

2. **Check checklists status** (if FEATURE_DIR/checklists/ exists):
   - Scan all checklist files in the checklists/ directory
   - For each checklist, count:
     - Total items: All lines matching `- [ ]` or `- [X]` or `- [x]`
     - Completed items: Lines matching `- [X]` or `- [x]`
     - Incomplete items: Lines matching `- [ ]`
   - Create a status table:

     ```text
     | Checklist | Total | Completed | Incomplete | Status |
     |-----------|-------|-----------|------------|--------|
     | ux.md     | 12    | 12        | 0          | ✓ PASS |
     | test.md   | 8     | 5         | 3          | ✗ FAIL |
     | security.md | 6   | 6         | 0          | ✓ PASS |
     ```

   - Calculate overall status:
     - **PASS**: All checklists have 0 incomplete items
     - **FAIL**: One or more checklists have incomplete items

   - **If any checklist is incomplete**:
     - Display the table with incomplete item counts
     - **STOP** and ask: "Some checklists are incomplete. Do you want to proceed with implementation anyway? (yes/no)"
     - Wait for user response before continuing
     - If user says "no" or "wait" or "stop", halt execution
     - If user says "yes" or "proceed" or "continue", proceed to step 3

   - **If all checklists are complete**:
     - Display the table showing all checklists passed
     - Automatically proceed to step 3

3. Load and analyze the implementation context:
   - **REQUIRED**: Read tasks.md for the complete task list and execution plan
   - **REQUIRED**: Read plan.md for tech stack, architecture, and file structure
   - **IF EXISTS**: Read data-model.md for entities and relationships
   - **IF EXISTS**: Read contracts/ for API specifications and test requirements
   - **IF EXISTS**: Read research.md for technical decisions and constraints
   - **IF EXISTS**: Read quickstart.md for integration scenarios

4. **Project Setup Verification**:
   - **REQUIRED**: Create/verify ignore files based on actual project setup:

   **Detection & Creation Logic**:
   - Check if the following command succeeds to determine if the repository is a git repo (create/verify .gitignore if so):

     ```sh
     git rev-parse --git-dir 2>/dev/null
     ```

   - Check if Dockerfile* exists or Docker in plan.md → create/verify .dockerignore
   - Check if .eslintrc* exists → create/verify .eslintignore
   - Check if eslint.config.* exists → ensure the config's `ignores` entries cover required patterns
   - Check if .prettierrc* exists → create/verify .prettierignore
   - Check if .npmrc or package.json exists → create/verify .npmignore (if publishing)
   - Check if terraform files (*.tf) exist → create/verify .terraformignore
   - Check if .helmignore needed (helm charts present) → create/verify .helmignore

   **If ignore file already exists**: Verify it contains essential patterns, append missing critical patterns only
   **If ignore file missing**: Create with full pattern set for detected technology

   **Common Patterns by Technology** (from plan.md tech stack):
   - **Node.js/JavaScript/TypeScript**: `node_modules/`, `dist/`, `build/`, `*.log`, `.env*`
   - **Python**: `__pycache__/`, `*.pyc`, `.venv/`, `venv/`, `dist/`, `*.egg-info/`
   - **Java**: `target/`, `*.class`, `*.jar`, `.gradle/`, `build/`
   - **C#/.NET**: `bin/`, `obj/`, `*.user`, `*.suo`, `packages/`
   - **Go**: `*.exe`, `*.test`, `vendor/`, `*.out`
   - **Ruby**: `.bundle/`, `log/`, `tmp/`, `*.gem`, `vendor/bundle/`
   - **PHP**: `vendor/`, `*.log`, `*.cache`, `*.env`
   - **Rust**: `target/`, `debug/`, `release/`, `*.rs.bk`, `*.rlib`, `*.prof*`, `.idea/`, `*.log`, `.env*`
   - **Kotlin**: `build/`, `out/`, `.gradle/`, `.idea/`, `*.class`, `*.jar`, `*.iml`, `*.log`, `.env*`
   - **C++**: `build/`, `bin/`, `obj/`, `out/`, `*.o`, `*.so`, `*.a`, `*.exe`, `*.dll`, `.idea/`, `*.log`, `.env*`
   - **C**: `build/`, `bin/`, `obj/`, `out/`, `*.o`, `*.a`, `*.so`, `*.exe`, `Makefile`, `config.log`, `.idea/`, `*.log`, `.env*`
   - **Swift**: `.build/`, `DerivedData/`, `*.swiftpm/`, `Packages/`
   - **R**: `.Rproj.user/`, `.Rhistory`, `.RData`, `.Ruserdata`, `*.Rproj`, `packrat/`, `renv/`
   - **Universal**: `.DS_Store`, `Thumbs.db`, `*.tmp`, `*.swp`, `.vscode/`, `.idea/`

   **Tool-Specific Patterns**:
   - **Docker**: `node_modules/`, `.git/`, `Dockerfile*`, `.dockerignore`, `*.log*`, `.env*`, `coverage/`
   - **ESLint**: `node_modules/`, `dist/`, `build/`, `coverage/`, `*.min.js`
   - **Prettier**: `node_modules/`, `dist/`, `build/`, `coverage/`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`
   - **Terraform**: `.terraform/`, `*.tfstate*`, `*.tfvars`, `.terraform.lock.hcl`
   - **Kubernetes/k8s**: `*.secret.yaml`, `secrets/`, `.kube/`, `kubeconfig*`, `*.key`, `*.crt`

5. Parse tasks.md structure and extract:
   - **Task phases**: Setup, Tests, Core, Integration, Polish
   - **Task dependencies**: Sequential vs parallel execution rules
   - **Task details**: ID, description, file paths, parallel markers [P]
   - **Execution flow**: Order and dependency requirements

6. Execute implementation following the task plan:
   - **Phase-by-phase execution**: Complete each phase before moving to the next
   - **Respect dependencies**: Run sequential tasks in order, parallel tasks [P] can run together
   - **Follow TDD approach**: Execute test tasks before their corresponding implementation tasks
   - **File-based coordination**: Tasks affecting the same files must run sequentially
   - **Validation checkpoints**: Verify each phase completion before proceeding

7. Implementation execution rules:
   - **Setup first**: Initialize project structure, dependencies, configuration
   - **Tests before code**: If you need to write tests for contracts, entities, and integration scenarios
   - **Core development**: Implement models, services, CLI commands, endpoints
   - **Integration work**: Database connections, middleware, logging, external services
   - **Polish and validation**: Unit tests, performance optimization, documentation

8. Progress tracking and error handling:
   - Report progress after each completed task
   - If a task fails, attempt to fix the issue before proceeding
   - For parallel tasks [P], continue with successful tasks, then return to fix failed ones
   - Provide clear error messages with context for debugging
   - If genuinely blocked (missing info, conflicting requirements), state `BLOCKER: [issue]` and request specific user decision
   - **IMPORTANT** For completed tasks, mark the task as [X] in tasks.md immediately after completion

9. Completion validation:
   - Verify ALL tasks are marked [X] complete - no exceptions
   - Check that implemented features match the original specification
   - Validate that tests pass and coverage meets requirements
   - Confirm the implementation follows the technical plan
   - Verify no prohibited code markers exist in generated files
   - Report final status with summary of completed work

**CRITICAL**: Implementation is not complete until all tasks are marked [X]. Partial completion is not an acceptable end state.

Note: This command assumes a complete task breakdown exists in tasks.md. If tasks are incomplete or missing, run `/speckit.tasks` first to generate the task list.

## MANDATORY: pgrx Background Worker Testing Requirement

Extensions with background workers MUST include a `pg_test` module at crate root with `postgresql_conf_options()`:

```rust
// MANDATORY - Must be at crate root (src/lib.rs)
// WITHOUT THIS MODULE, BACKGROUND WORKER TESTS WILL FAIL
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='extension_name'"]
    }
}
```

**Why this is MANDATORY**:
- pgrx-tests framework calls `crate::pg_test::postgresql_conf_options()` during test initialization
- The returned settings are written to `postgresql.auto.conf` BEFORE PostgreSQL starts
- Background workers can ONLY be registered during `shared_preload_libraries` loading
- Without this module, background worker tests WILL FAIL

**Implementation MUST verify**:
1. `pg_test` module exists at crate root
2. `postgresql_conf_options()` returns the extension name in `shared_preload_libraries`
3. Delete `target/test-pgdata/` before running tests if config changed
4. Verify `postgresql.auto.conf` contains correct settings

## No False Impossibility Claims (Constitution XVI)

Claiming that tests or implementations are "impossible" is PROHIBITED. You have full source code access.

**You have NO excuse for claiming impossibility:**
- `/Users/brandon/src/pgrx/` - Full pgrx source code with examples and tests
- `/Users/brandon/src/postgres/` - Full PostgreSQL source code with implementation details
- `pg_settings` system catalog with `min_val`, `max_val`, `vartype`, `context` columns

**The test/implementation is NEVER impossible. The approach is wrong. Fix the approach.**

## pgrx Reference

**Local pgrx Repository**: `/Users/brandon/src/pgrx/`
- Consult this repository for pgrx API patterns, examples, and best practices during implementation
- Key directories:
  - `pgrx/` - Core framework code
  - `pgrx-examples/` - Example extensions demonstrating patterns
  - `pgrx-macros/` - Procedural macro implementations
  - `pgrx-pg-sys/` - PostgreSQL bindings

## PostgreSQL Reference

**Local PostgreSQL Source**: `/Users/brandon/src/postgres/`
- Consult this repository for PostgreSQL internal APIs, struct definitions, and implementation details
- Key directories for extension development:
  - `src/backend/postmaster/checkpointer.c` - Checkpointer process implementation
  - `src/backend/postmaster/bgworker.c` - Background worker infrastructure
  - `src/backend/utils/misc/guc.c` - GUC (Grand Unified Configuration) system
  - `src/include/pgstat.h` - Statistics collector definitions
  - `src/backend/commands/variable.c` - ALTER SYSTEM implementation
