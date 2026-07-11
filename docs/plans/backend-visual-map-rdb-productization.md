# Backend Visual Map RDB Productization Plan

Status: Complete (Phases 31-40 implemented; live engine smoke blocked locally)
Scale: Large
Parent plan: `docs/plans/backend-visual-map.md`

## Goal

Make every RDB adapter already supported by `database-memory` usable from the Backend Visual Map product path.

The finished product lets a user create one active DB profile for SQLite, SQLite DDL, PostgreSQL, MySQL/MariaDB, SQL Server, or Oracle, index it through the bundled `database-memory.exe`, load its inventory, and render it in the visual map without fake data or secret persistence.

## Current Facts

- `database-memory` supports `sqlite`, `ddl-sqlite`, `postgres`, `mysql`, `sqlserver`, and `oracle`.
- Backend Visual Map's Rust `DbSource` enum already names all six sources.
- Backend Visual Map's current UI state and save request only expose `sqlite` and `ddl-sqlite`.
- Backend Visual Map's current DB engine argument mapping rejects non-SQLite sources.
- `database-memory index` uses:
  - `--source <source>`
  - `--path <path>` for SQLite file and SQLite DDL sources
  - `--connection-string <secret>` for network RDB sources
  - `--alias <profile-id>`
  - `--cache-path <workspace-db-cache>`
- Product v1 does not store passwords or connection strings.
- Product v1 must remain metadata-only and must not read DB row data.

## Proposed Behavior

- Source selector lists all six supported RDB sources.
- SQLite and SQLite DDL profiles store only a local path.
- PostgreSQL, MySQL/MariaDB, SQL Server, and Oracle profiles accept a session-only connection string at index time.
- Connection strings are never written to `workspace.json`, app data snapshots, scan logs, frontend persisted state, inspector text, or errors.
- Non-secret display metadata may be stored when it is explicitly entered or safely derived without storing credentials.
- Indexing uses the same existing sidecar runner and scan-event flow.
- Inventory loading and visual map generation work from the graph cache after indexing, independent of source type.
- Failure states distinguish missing engine, missing path, missing connection string, connection failure, and empty graph.

## Success Criteria

- A user can create and index profiles for:
  - SQLite file
  - SQLite DDL path
  - PostgreSQL connection string
  - MySQL/MariaDB connection string
  - SQL Server ADO connection string
  - Oracle `user/password@connect_string`
- The app stores no connection string or password after indexing.
- Existing SQLite and DDL SQLite flows continue to work.
- `npm run typecheck`, `npm run build`, and `cargo test` pass.
- Available local smoke tests are recorded in phase reports.
- Unsupported live DB environments skip clearly instead of failing the whole product check.

## Non-Goals

- No DB row reads.
- No arbitrary SQL console.
- No password persistence.
- No OS credential store integration in this plan.
- No multi-DB simultaneous visual map.
- No new database adapter implementation inside Backend Visual Map.
- No changes to `database-memory` unless a real engine contract bug is found.
- No fake/demo fallback for product indexing or product map rendering.
- No migration execution.

## Source Input Matrix

| Source | User Input | Stored In Profile | Secret? | Engine Args |
| --- | --- | --- | --- | --- |
| `sqlite` | SQLite database file path | `path` | No | `--source sqlite --path <path>` |
| `ddl-sqlite` | SQL file or migration directory path | `path` | No | `--source ddl-sqlite --path <path>` |
| `postgres` | PostgreSQL URL | No raw connection string | Yes | `--source postgres --connection-string <value>` |
| `mysql` | MySQL/MariaDB URL | No raw connection string | Yes | `--source mysql --connection-string <value>` |
| `sqlserver` | ADO.NET-style connection string | No raw connection string | Yes | `--source sqlserver --connection-string <value>` |
| `oracle` | `user/password@connect_string` | No raw connection string | Yes | `--source oracle --connection-string <value>` |

## Architecture

Use the existing boundaries. Do not add a new abstraction unless an existing function cannot support the product path.

```text
React Workbench DB Panel
  -> SaveDbProfileRequest
  -> Tauri save_db_profile
  -> workspace.json without secrets

React Index DB action
  -> IndexDbProfileRequest + session connection string if required
  -> Tauri index_db_profile
  -> database-memory.exe index
  -> workspace db/<profile-id>/graph.sqlite
  -> load inventory
  -> save atlas inventory snapshot
  -> visual map
```

## Implementation Phases

### Phase 31: RDB Profile Contract Audit

Status: Complete (implemented 2026-07-06)

Goal:

- Lock the source/input/engine contract before touching UI.

Deliverables:

- Add this addendum to the parent plan's implementation prompt section.
- Verify the current `database-memory` CLI contract from README/source/help.
- Document any mismatch between Backend Visual Map and `database-memory`.
- Do not change runtime behavior in this phase unless a compile-breaking doc reference is found.

Verification:

```powershell
rg -n "SaveDbProfileRequest|DbProfile|db_index_args|db_cli_source|connection-string|ddl-sqlite|postgres|mysql|sqlserver|oracle" src src-tauri docs
```

Report:

- `docs/reports/backend-visual-map.phase-31.md`

Rollback:

- Remove this addendum link if the scope is rejected.

### Phase 32: Shared DB Profile Request Contract

Status: Complete (implemented 2026-07-06)

Goal:

- Allow the frontend and Rust commands to express all supported RDB sources without storing secrets.

Deliverables:

- Expand TypeScript `SaveDbProfileRequest` source type to all six sources.
- Add session-only connection string to the index request, not to persisted `DbProfile`.
- Update Rust `IndexDbProfileRequest` with optional `connection_string`.
- Keep `DbProfile.passwordStored` false.
- Keep persisted profile data secret-free.

Verification:

```powershell
npm run typecheck
cargo test
```

Required tests:

- Saving a network DB profile does not persist a connection string.
- Saved workspace JSON does not contain a fixture password.

Report:

- `docs/reports/backend-visual-map.phase-32.md`

Rollback:

- Revert request type expansion; SQLite paths remain usable.

### Phase 33: Source-Aware DB Engine Args

Status: Complete (implemented 2026-07-06)

Goal:

- Make `index_db_profile` call `database-memory` correctly for every supported source.

Deliverables:

- `db_cli_source` returns all six database-memory source strings.
- `db_index_args` uses `--path` for `sqlite` and `ddl-sqlite`.
- `db_index_args` uses `--connection-string` for `postgres`, `mysql`, `sqlserver`, and `oracle`.
- Missing path and missing connection string produce readable errors before starting the sidecar.
- Redaction is applied to sidecar stdout/stderr and scan events.

Verification:

```powershell
cargo test
```

Required tests:

- One args test per source.
- Missing connection string fails before sidecar execution.
- Redaction test includes URL, ADO, and Oracle-style credentials.

Report:

- `docs/reports/backend-visual-map.phase-33.md`

Rollback:

- Restore SQLite-only mapping.

### Phase 34: RDB Profile UI

Status: Complete (implemented 2026-07-06)

Goal:

- Let users create and index every supported source from the Workbench.

Deliverables:

- Workbench source selector includes:
  - SQLite
  - SQLite DDL
  - PostgreSQL
  - MySQL/MariaDB
  - SQL Server
  - Oracle
- Input label changes by selected source.
- SQLite sources show a path input.
- Network sources show a session-only secret input.
- Saved/opened network profiles do not repopulate the secret input.
- Copy makes the non-storage rule clear without adding a separate settings flow.

Verification:

```powershell
npm run typecheck
npm run build
```

Report:

- `docs/reports/backend-visual-map.phase-34.md`

Rollback:

- Hide non-SQLite options from the selector.

### Phase 35: Source-Aware Inventory And Status UX

Status: Complete (implemented 2026-07-06)

Goal:

- Remove SQLite-only wording from the product path and make source state visible.

Deliverables:

- Workbench, Atlas, status bar, and inspector show the active DB source.
- Empty/loading/error states work for every source.
- Error messages never echo connection strings.
- `loadInventory` and visual-map snapshot persistence remain source-agnostic.
- Existing no-fake-data behavior stays intact.

Verification:

```powershell
npm run typecheck
npm run build
cargo test
```

Report:

- `docs/reports/backend-visual-map.phase-35.md`

Rollback:

- Restore previous labels and SQLite-only UI.

### Phase 36: Local Smoke Commands

Status: Complete (implemented 2026-07-06)

Goal:

- Provide repeatable smoke checks without requiring every RDB server on every machine.

Deliverables:

- Add a small smoke script or documented command set for:
  - SQLite
  - SQLite DDL
  - PostgreSQL when `BACKEND_MAP_TEST_POSTGRES_URL` is set
  - MySQL when `BACKEND_MAP_TEST_MYSQL_URL` is set
  - SQL Server when `BACKEND_MAP_TEST_SQLSERVER_URL` is set
  - Oracle when `BACKEND_MAP_TEST_ORACLE_URL` is set
- Missing env vars skip clearly.
- No script prints connection strings.

Verification:

```powershell
npm run typecheck
npm run build
cargo test
```

Also run every smoke path available on the current machine and record skips.

Report:

- `docs/reports/backend-visual-map.phase-36.md`

Rollback:

- Keep manual docs only and remove the script.

### Phase 37: Real Project Smoke With meeting-overlay-assistant

Status: Complete (implemented 2026-07-06; live product smoke blocked by missing engines)

Goal:

- Prove the product path on a real large backend repository.

Deliverables:

- Use `shinyeonjun/meeting-overlay-assistant` as the codebase smoke target.
- Index the repo through the product's code engine path if the engine binary is available.
- Add one DB profile using the best available schema source:
  - live PostgreSQL if a test DB is available
  - otherwise documented DDL/schema source if present
  - otherwise mark DB live smoke as blocked, not passed
- Load inventories and generate a visual map from real data only.

Verification:

```powershell
npm run typecheck
npm run build
cargo test
```

Manual verification:

- `npm run tauri dev` product smoke when sidecar binaries are present.

Report:

- `docs/reports/backend-visual-map.phase-37.md`

Rollback:

- Remove only smoke artifacts; do not revert product code.

### Phase 38: Secret Persistence Regression

Status: Complete (implemented 2026-07-06)

Goal:

- Make secret leakage hard to reintroduce.

Deliverables:

- Add tests or scripted checks that search app data/workspace fixtures for fixture secrets.
- Confirm scan events, workspace files, atlas snapshots, and UI-visible errors are redacted.
- Add/extend tests for:
  - PostgreSQL URL password
  - MySQL URL password
  - SQL Server `Password=` and `Pwd=`
  - Oracle `user/password@connect_string`

Verification:

```powershell
cargo test
npm run typecheck
npm run build
```

Report:

- `docs/reports/backend-visual-map.phase-38.md`

Rollback:

- Remove only the new checks if they are too brittle; keep runtime redaction.

### Phase 39: Product UX Cleanup

Status: Complete (implemented 2026-07-06)

Goal:

- Make the multi-RDB flow feel like one product, not bolted-on options.

Deliverables:

- Remove stale SQLite-only wording.
- Tighten labels and empty states.
- Keep all controls within the existing dense desktop layout.
- Do not add new features.
- Do not add fake content.

Verification:

```powershell
npm run typecheck
npm run build
```

Manual verification:

- Workbench at 1440x900.
- Atlas at 1440x900.

Report:

- `docs/reports/backend-visual-map.phase-39.md`

Rollback:

- Revert CSS/copy-only changes.

### Phase 40: Final RDB Product Review

Status: Complete (implemented 2026-07-06)

Goal:

- Decide whether the multi-RDB product path is release-candidate quality.

Deliverables:

- Review current diff against this plan.
- Run all available automated checks.
- Run all available smoke checks.
- Update final checklist with pass/skip/block status.
- Mark this addendum complete only if product paths are real and secret-safe.

Verification:

```powershell
npm run typecheck
npm run build
cargo test
```

Report:

- `docs/reports/backend-visual-map.phase-40.md`

Rollback:

- Mark the addendum partial and list blockers.

## Test Plan

Minimum automated checks:

```powershell
cd D:\project\backend_map
npm run typecheck
npm run build
cd src-tauri
cargo test
```

Minimum product smoke:

```powershell
cd D:\project\backend_map
npm run tauri dev
```

Minimum DB engine smoke:

```powershell
database-memory index --source ddl-sqlite --path D:\project\db_mcp\examples\sample-schema.sql --alias shop --cache-path <temp>\shop.sqlite
database-memory find-table ddl-sqlite:shop order --cache-path <temp>\shop.sqlite --format json
```

## Risks And Assumptions

- Network DB live smoke depends on local services, Docker containers, or user-provided connection strings.
- Oracle live smoke depends on Oracle Instant Client and an Oracle database.
- `database-memory` adapter support is treated as the source of truth; Backend Visual Map should not duplicate adapter logic.
- Connection strings are required at index time for network DBs because v1 intentionally avoids password storage.
- A source may index successfully but expose limited capability warnings; the product should display those warnings instead of hiding them.

## Codex CLI Prompt

```text
Read D:\project\backend_map\docs\plans\backend-visual-map.md and D:\project\backend_map\docs\plans\backend-visual-map-rdb-productization.md.

Implement the RDB productization addendum starting at Phase 31.

Rules:
- Implement phases in order.
- Do not implement later phases early.
- Keep patches small.
- Reuse existing workspace, engine, scan, inventory, and visual map flows.
- Do not add fake/demo fallback.
- Do not read DB row data.
- Do not add arbitrary SQL execution.
- Do not store passwords or connection strings.
- For every phase, write docs/reports/backend-visual-map.phase-N.md.
- Update docs/plans/backend-visual-map-rdb-productization.md phase status as work completes.
- Stop if tests fail and fix before continuing.

Run the checks listed in each phase.
```

## Review Prompt

```text
You did not implement this phase. Review the current diff only against D:\project\backend_map\docs\plans\backend-visual-map-rdb-productization.md.

Findings first, ordered by severity.
Focus on product-path bugs, secret persistence, row-data access, fake data fallback, engine CLI contract mismatch, missing tests, and phase-scope creep.
Do not modify files.
```
