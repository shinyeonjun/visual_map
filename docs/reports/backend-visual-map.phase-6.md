# Backend Visual Map Phase 6 Report

Date: 2026-07-06
Status: Complete

## Scope

Implemented Phase 6 only: engine registry contract, dev/production engine path resolution, a Tauri command that reports engine availability, and minimal UI status indicators.

## Changed Files

- `src-tauri/src/engine.rs`
  - Added `EngineRegistry`, `EngineAvailability`, and `EngineRuntimeMode`.
  - Defined expected code and DB engine executables:
    - `codebase-memory-mcp.exe`
    - `database-memory.exe`
  - Added dev/production/override engine directory resolution.
  - Added file-existence availability checks without running engines.
  - Added Rust unit tests for dev/prod path resolution and availability.
- `src-tauri/src/lib.rs`
  - Added `get_engine_availability` Tauri command.
  - Registered the command in the invoke handler.
- `src/types/engine.ts`
  - Added frontend engine registry types.
- `src/App.tsx`
  - Invokes `get_engine_availability` and passes the result to views.
- `src/components/WorkbenchView.tsx`
  - Replaced hardcoded engine OK status with registry-backed available/missing/error states.
- `src/components/AtlasView.tsx`
  - Added compact code/DB engine status chips in the existing shell footer area.
- `src/App.css`
  - Added status styling for available, missing, and error engine states.
- `docs/plans/backend-visual-map.md`
  - Marked Phase 6 complete and updated top-level status.

## Checks

- `cargo fmt`
  - Passed.
- `cargo test`
  - Passed: 11 tests.
- `npm run typecheck`
  - Passed.
- `npm run build`
  - Passed.
- `npm run tauri dev`
  - Vite started on `http://localhost:1420/`.
  - Tauri dev build completed and launched `target\debug\backend-visual-map.exe`.
  - Stopped manually with Ctrl+C after launch verification; resulting interrupt exit was expected.

## Results

- The app can now ask Rust for engine availability through `get_engine_availability`.
- Dev mode resolves engines under the app-local `engines` directory.
- Production mode resolves engines under the Tauri resource `engines` directory, with executable-directory fallback.
- `BACKEND_VISUAL_MAP_ENGINE_DIR` can override the engine directory for local setup/testing.
- The UI shows code and DB engine states as available, missing, or error.

## Skipped Work

- Did not run engine executables.
- Did not add indexing.
- Did not add repo scan.
- Did not add DB connection.
- Did not add sidecar runner or timeout handling.
- Did not add Tauri `externalBin` bundling.
- Did not add React Flow or graph data.
