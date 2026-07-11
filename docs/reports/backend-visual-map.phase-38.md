# Backend Visual Map Phase 38 Report

Date: 2026-07-06
Status: Complete

## Scope

Added secret persistence regressions for multi-RDB connection string shapes.

## Changed Files

- `src-tauri/src/atlas.rs`
- `src-tauri/src/workspace.rs`
- `docs/plans/backend-visual-map-rdb-productization.md`
- `docs/reports/backend-visual-map.phase-38.md`

## Implementation

- Inventory snapshot persistence now applies common secret redaction before writing `inventory-snapshot.json`.
- Added an atlas snapshot persistence test covering:
  - PostgreSQL URL password
  - SQL Server `Password=`
  - SQL Server `Pwd=`
  - Oracle `user/password@connect_string`
- Expanded scan event persistence test to cover PostgreSQL URL, SQL Server, and Oracle secret shapes.
- Existing network profile save regression continues to prove `workspace.json` does not persist a PostgreSQL URL-shaped fixture secret.
- Existing frontend DB error display redacts the exact current session connection string before showing it.

## Checks

- `cargo fmt`
  - Passed.
- `cargo test`
  - Passed: 32 tests.
- `npm run typecheck`
  - Passed.
- `npm run build`
  - Passed.

## Skipped Work

- No browser-level UI error redaction test was added because no frontend test runner is configured.
- No live DB smoke because local engines are unavailable.
