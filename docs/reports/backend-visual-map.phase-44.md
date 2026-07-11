# Backend Visual Map Phase 44

Date: 2026-07-06

## Changed Files

- `src-tauri/src/workspace/model.rs`
- `src-tauri/src/workspace/store.rs`
- `src-tauri/src/workspace/code.rs`
- `src-tauri/src/workspace/db.rs`
- `src-tauri/src/workspace/mod.rs`
- `src-tauri/src/workspace/tests.rs`
- `src/types/workspace.ts`

## Results

- Added workspace engine cache metadata to `workspace.json`.
- Code engine indexing now creates and passes a workspace-scoped cache path.
- Code inventory queries include the same workspace-scoped cache path in their payloads.
- DB cache metadata stays workspace-scoped under the existing per-workspace DB directory.
- Added tests for code cache and DB cache path derivation.

## Checks

- `cargo test` passed: 42 tests.
- `npm run typecheck` passed.
- Engine path smoke skipped: no `codebase-memory-mcp.exe` or `database-memory.exe` was present under `D:\project\backend_map`.

## Skipped Work

- Did not change engine binaries or package sidecars.
- Did not add global MCP config registration.
- Did not add DB row access or password persistence.

## Risks

- The code engine cache path is passed through the existing JSON payload contract; if a future sidecar requires a different cache flag, Phase 45/engine contract work should adapt the centralized argument builder.
