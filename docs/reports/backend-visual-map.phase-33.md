# Backend Visual Map Phase 33 Report

Date: 2026-07-06
Status: Complete

## Scope

Made DB indexing engine args source-aware for all `database-memory` supported RDB sources.

## Changed Files

- `src-tauri/src/workspace.rs`
- `src-tauri/src/engine.rs`
- `docs/plans/backend-visual-map-rdb-productization.md`
- `docs/reports/backend-visual-map.phase-33.md`

## Implementation

- `db_cli_source` now maps all six sources: `sqlite`, `ddl-sqlite`, `postgres`, `mysql`, `sqlserver`, and `oracle`.
- `db_index_args` emits `--path` for SQLite and DDL SQLite sources.
- `db_index_args` emits `--connection-string` for PostgreSQL, MySQL/MariaDB, SQL Server, and Oracle.
- `index_db_profile` validates missing network connection strings before checking engine availability or starting a sidecar.
- Secret redaction now covers PostgreSQL/MySQL URLs, SQL Server `Password=`/`Pwd=`, and Oracle `user/password@connect_string`.

## Checks

- `cargo fmt`
  - Passed.
- `cargo test`
  - Passed: 31 tests.
  - During the first run, the new Oracle redaction test exposed an infinite re-redaction loop. The hung test process was stopped, the scanner was fixed to advance past `@`, and the full suite then passed.
- `npm run typecheck`
  - Passed.
- `npm run build`
  - Passed.

## Skipped Work

- No UI source selector changes in this phase; that remains Phase 34.
- No live DB indexing smoke because `database-memory` is unavailable locally.
