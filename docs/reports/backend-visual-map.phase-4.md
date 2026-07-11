# Phase 4: Workspace Data Model

Status: Complete

## Changed Files

- `package.json`
- `package-lock.json`
- `src/types/workspace.ts`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/lib.rs`
- `src-tauri/src/workspace.rs`
- `docs/plans/backend-visual-map.md`
- `docs/reports/backend-visual-map.phase-4.md`

## What Was Implemented

- Added Rust `Workspace`, `DbProfile`, and `DbSource` contracts.
- Added TypeScript workspace and DB profile contracts.
- Added JSON serialization/deserialization using camelCase fields.
- Added workspace ID generation.
- Added `workspace.json` write/read under `workspaces/<workspace-id>/`.
- Added Tauri commands:
  - `create_workspace`
  - `open_workspace`
  - `list_workspaces`

## Deliberately Skipped

- Workspace selection UI.
- Recent workspace UI.
- App-state SQLite schema or persistence.
- Repo scanning, DB connections, engine registry, sidecars, or engine execution.
- Inventory, candidate links, visual map contracts, or React Flow.

## Commands Run

```powershell
npm install @tauri-apps/api@^2
cargo fmt
cargo test
npm run typecheck
cargo check
npm run tauri dev
```

## Check Results

- `cargo test`: passed. 4 tests passed, including workspace serialization and create/open/list round trip.
- `npm run typecheck`: passed.
- `cargo check`: passed.
- `npm run tauri dev`: passed compile and launched `target\debug\backend-visual-map.exe`.

Note: `npm run tauri dev` was stopped with Ctrl-C after successful launch, so the wrapper process ended with `STATUS_CONTROL_C_EXIT`.

## Known Risks

- Workspace IDs are generated from a slug, epoch milliseconds, and process ID. This avoids adding a UUID dependency for Phase 4.
- Commands are available but not wired to user-facing UI until Phase 5.
- `app-state.sqlite` remains uncreated; this phase only writes per-workspace `workspace.json`.

## Review Fix

- Added `workspace_id` validation before `open_workspace` builds a filesystem path.
- Allowed only non-empty ASCII alphanumeric IDs plus `-`.
- Added tests for `../x` path traversal rejection and empty ID rejection.
- Re-ran `cargo test` and `npm run typecheck`; both passed.

## Next Recommended Phase

Phase 5: Workspace UI.
