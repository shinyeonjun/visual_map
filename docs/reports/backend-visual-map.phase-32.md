# Backend Visual Map Phase 32 Report

Date: 2026-07-06
Status: Complete

## Scope

Expanded the shared DB profile request contract so every supported RDB source can be represented without persisting secrets.

## Changed Files

- `src-tauri/src/workspace.rs`
- `src/types/workspace.ts`
- `docs/plans/backend-visual-map-rdb-productization.md`
- `docs/reports/backend-visual-map.phase-32.md`

## Implementation

- Added optional `connectionString`/`connection_string` to the DB index request only.
- Expanded the TypeScript `SaveDbProfileRequest.source` contract to all six database sources.
- Kept persisted `DbProfile` secret-free.
- Updated Rust profile saving so network DB profiles never persist a raw path-like connection string.
- Added a regression test proving a PostgreSQL URL-shaped fixture secret is not written to `workspace.json`.

## Checks

- `cargo fmt`
  - Passed.
- `npm run typecheck`
  - Passed.
- `npm run build`
  - Passed.
- `cargo test`
  - Passed: 24 tests.
  - Warning: `IndexDbProfileRequest.connection_string` is unused until Phase 33 connects it to engine args.

## Skipped Work

- UI source selection remains SQLite-only until Phase 34.
- Engine args remain SQLite-only until Phase 33.
