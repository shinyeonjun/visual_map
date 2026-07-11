# Backend Visual Map Refactor: Rust Workspace Module

Date: 2026-07-06

## Changed Files

- `src-tauri/src/workspace/mod.rs`
- `src-tauri/src/workspace/model.rs`
- `src-tauri/src/workspace/store.rs`
- `src-tauri/src/workspace/code.rs`
- `src-tauri/src/workspace/db.rs`
- `src-tauri/src/workspace/events.rs`
- `src-tauri/src/workspace/tests.rs`
- Removed `src-tauri/src/workspace.rs`
- Moved `src-tauri/src/workspace_tests.rs` into the workspace module folder.

## Results

- Split workspace types, store functions, code indexing/query functions, DB profile/inventory functions, and scan events into separate Rust modules.
- Kept public function names and Tauri call sites unchanged.
- Preserved existing tests with test-only helper re-exports.

## Checks

- `cargo test` passed: 40 tests.
- `npm run typecheck` passed.
- `npm run build` passed.

## Skipped

- No behavior, command surface, engine integration, DB access pattern, or workspace file format changed.
