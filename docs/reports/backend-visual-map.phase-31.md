# Backend Visual Map Phase 31 Report

Date: 2026-07-06
Status: Complete

## Scope

RDB profile contract audit before UI/runtime changes.

## Findings

- The addendum contract expects `database-memory index --source <source>` with `--path <path>` for `sqlite` and `ddl-sqlite`, and `--connection-string <value>` for `postgres`, `mysql`, `sqlserver`, and `oracle`.
- Local `database-memory` CLI/source verification was unavailable on this machine: `database-memory` was not on PATH and `D:\db_mcp` did not exist.
- Current Backend Visual Map mismatch:
  - TypeScript `SaveDbProfileRequest` and UI controls only expose `sqlite` and `ddl-sqlite`.
  - Rust `DbSource` already models all six sources.
  - Rust `db_cli_source` rejects non-SQLite sources.
  - Rust `IndexDbProfileRequest` has no session-only connection string field.
  - Current persisted `DbProfile` contract remains secret-free with `passwordStored: false`.

## Changed Files

- `docs/plans/backend-visual-map.md`
- `docs/plans/backend-visual-map-rdb-productization.md`
- `docs/reports/backend-visual-map.phase-31.md`

## Checks

- `Get-Command database-memory -ErrorAction SilentlyContinue`
  - Unavailable.
- `Test-Path -LiteralPath D:\db_mcp`
  - Returned `False`.
- `rg -n "SaveDbProfileRequest|DbProfile|db_index_args|db_cli_source|connection-string|ddl-sqlite|postgres|mysql|sqlserver|oracle" src src-tauri docs`
  - Passed and confirmed the mismatches above.

## Skipped Work

- No runtime behavior changed in this audit phase.
- Direct `database-memory --help` verification was skipped because the CLI is unavailable locally.
