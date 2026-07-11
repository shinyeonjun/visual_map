# Phase 3: App Path And Workspace Directory

Status: Complete

## Changed Files

- `package.json`
- `package-lock.json`
- `src/App.tsx`
- `src/App.css`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/lib.rs`
- `src-tauri/src/paths.rs`
- `docs/plans/backend-visual-map.md`
- `docs/reports/backend-visual-map.phase-3.md`

## What Was Implemented

- Added Rust command `get_app_paths`.
- Added Rust path helper that derives:
  - app data directory
  - `app-state.sqlite` path
  - `engines` directory
  - `workspaces` directory
- Added directory creation for app data, `engines`, and `workspaces`.
- Added dev-only frontend diagnostics that displays resolved app paths.

## Deliberately Skipped

- Workspace model and `workspace.json` read/write.
- SQLite app-state schema or file creation.
- Engine registry, sidecars, or engine execution.
- Repo scan and DB connection behavior.
- Any Phase 4+ command surface.

## Commands Run

```powershell
npm install @tauri-apps/api@^2
cargo fmt
cargo test
npm run typecheck
cargo check
npm run tauri dev
npm run build
```

## Check Results

- `cargo test`: passed. 2 path helper tests passed.
- `npm run typecheck`: passed.
- `cargo check`: passed.
- `npm run tauri dev`: passed compile and launched `target\debug\backend-visual-map.exe`.
- `npm run build`: passed.

Note: `npm run tauri dev` was stopped with Ctrl-C after successful launch, so the wrapper process ended with `STATUS_CONTROL_C_EXIT`.

## Manual Run Result

Tauri resolved app data under:

```text
%APPDATA%\com.backendvisualmap.app
```

Created:

- `%APPDATA%\com.backendvisualmap.app\engines`
- `%APPDATA%\com.backendvisualmap.app\workspaces`

Returned but did not create yet:

- `%APPDATA%\com.backendvisualmap.app\app-state.sqlite`

## Known Risks

- Tauri's Windows `app_data_dir` resolved to Roaming app data in this environment, not `%LOCALAPPDATA%`.
- The app-state SQLite file is only a returned path in Phase 3; schema/file creation belongs to a later storage phase.

## Next Recommended Phase

Phase 4: Workspace Data Model.
