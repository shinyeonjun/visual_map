# Backend Visual Map Phase 71 Report

## Summary

Phase 71 built the Windows NSIS installer, installed it into a temporary directory, and verified the installed app can run without PATH-based engine lookup. The smoke exposed a real DB inventory bug, which was fixed before continuing.

## Changed Files

- `src-tauri/src/engine.rs`
  - Preserved empty command arguments when spawning engines. `database-memory find-table/find-column` requires an explicit empty query argument.
- `src-tauri/src/engine_tests.rs`
  - Added a regression test proving empty engine arguments are preserved.
- `src-tauri/src/workspace/db.rs`
  - Removed unsupported `--format json` from `database-memory find-table/find-column`.
  - Added line-output parsing for current `database-memory` table/column results.
- `src-tauri/src/workspace/mod.rs`
  - Re-exported DB line-output parser for tests.
- `src-tauri/src/workspace/tests.rs`
  - Updated DB find-arg contract tests.
  - Added line-output inventory parsing coverage.
- `scripts/tauri-cdp-smoke.mjs`
  - Added a small CDP smoke helper for installed Tauri app checks and screenshots.

## Checks

- PASS: `cargo fmt`
- PASS: `cargo test` - 55 tests passed.
- PASS: `npm run typecheck`
- PASS: `npm run build`
- PASS: `npm run tauri build`
- PASS: Silent NSIS install to `%TEMP%\backend-map-smoke-install`.
- PASS: Installed directory contained:
  - `backend-visual-map.exe`
  - `engines/codebase-memory-mcp.exe`
  - `engines/database-memory.exe`
  - `THIRD_PARTY_NOTICES.md`
  - `uninstall.exe`
- PASS: Installed app launched with `PATH` set to an empty string.
- PASS: Installed app end-to-end IPC smoke:
  - Created workspace `phase71-final-smoke-*`.
  - Indexed `D:\meeting-overlay-assistant`.
  - Loaded code inventory: 50 routes, 80 services, 12 files.
  - Saved SQLite DDL profile for the meeting-overlay schema.
  - Indexed DB metadata.
  - Loaded DB inventory: 18 tables, `sessions` had 12 columns.
  - Confirmed `passwordStored=false`.
- PASS: Silent uninstall returned exit code 0.
- PASS: Temporary install directory was removed.
- PASS: No new Codex or Claude global config files were created.

## Results

- The Windows installer can produce a runnable installed app.
- The installed app does not require Node, Rust, or PATH-based engine discovery for bundled engines.
- The installed app can create a workspace and index real code/DB metadata through the existing product commands.
- No DB row-data access was added.
- No DB passwords were persisted.
- No MCP auto-registration was added or run.

## Issue Found And Fixed

- `get_db_inventory` failed against the packaged `database-memory.exe` because:
  - `find-table/find-column` do not support `--format json`.
  - The app dropped the required empty query argument before spawning the engine.
- The fix keeps empty engine args and parses current line-based DB inventory output.

## Skipped Work

- A fully isolated Windows user profile was not available. The smoke used a temporary install directory and verified no new Codex/Claude global config files were added.
