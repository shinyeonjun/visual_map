# Backend Visual Map Phase 8 Report

Date: 2026-07-06
Status: Complete

## Scope

Implemented Phase 8 only: SQLite/DDL DB profile capture, workspace profile persistence, and a database metadata indexing command path through `database-memory.exe`.

## Changed Files

- `src-tauri/src/workspace.rs`
  - Added `SaveDbProfileRequest`, `IndexDbProfileRequest`, and `DbIndexResult`.
  - Added DB profile save/update under `workspace.json`.
  - Added DB cache path creation under `workspaces\<workspace-id>\db\<profile-id>\graph.sqlite`.
  - Added `index_db_profile` using the Phase 7 runner and `database-memory.exe index --format json`.
  - Added a test proving DB profile persistence does not store password data.
- `src-tauri/src/lib.rs`
  - Added `save_db_profile` and `index_db_profile` Tauri commands.
- `src/types/workspace.ts`
  - Added DB profile save/index request types.
- `src/types/workspaceControls.ts`
  - Added DB profile UI control contract.
- `src/App.tsx`
  - Wired DB profile save/index commands into app state.
- `src/components/WorkbenchView.tsx`
  - Replaced dummy DB profile label with minimal SQLite/DDL metadata profile form.
- `src/components/AtlasView.tsx`
  - Shows the active DB profile name when present.
- `src/App.css`
  - Added compact inline input/select styles.
- `docs/plans/backend-visual-map.md`
  - Marked Phase 8 complete.

## Checks

- `cargo fmt`
  - Passed.
- `cargo test`
  - Passed: 14 tests.
- `npm run typecheck`
  - Passed.
- `npm run build`
  - Passed.

## Results

- Users can save a SQLite or DDL SQLite profile without any password/token field.
- Users can request DB metadata indexing through the existing database engine registry and runner.
- If `database-memory.exe` is missing, the command returns a clear missing-engine error.

## Review Fix

- Fixed the `database-memory.exe index` argument contract after review:
  - Changed `--cache` to the actual CLI flag `--cache-path`.
  - Added required `--alias <profile-id>`.
- Added a regression test for the DB index CLI argument contract.
- Independent smoke against `D:\project\db_mcp\examples\sample-schema.sql` passed with `database-memory.exe index --source ddl-sqlite --alias local-ddl --cache-path <temp>`.

## Skipped Work

- Did not add external DB connection profiles.
- Did not store DB passwords or tokens.
- Did not inspect DB row data.
- Did not implement inventory query UI; that starts in Phase 9.
