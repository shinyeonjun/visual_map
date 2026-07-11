# Backend Visual Map Phase 70 Report

## Summary

Phase 70 packaged the two engine executables as internal Tauri release resources so the app does not depend on PATH for normal release lookup.

## Changed Files

- `src-tauri/tauri.conf.json`
  - Added release resources for `engines/codebase-memory-mcp.exe` and `engines/database-memory.exe`.
  - Kept engines as internal bundled resources, not MCP registrations.
- `src-tauri/engines/codebase-memory-mcp.exe`
  - Added bundled code engine binary copied from the app-local engine cache.
- `src-tauri/engines/database-memory.exe`
  - Added bundled DB metadata engine binary copied from the app-local engine cache.
- `src-tauri/sidecars/external-bin.config.json`
  - Updated the reference packaging shape from stale `sidecars/...` external-bin names to `engines/...` resources.
- `src-tauri/sidecars/README.md`
  - Updated packaging notes to describe `src-tauri/engines` resources and explicitly forbid installer/setup/MCP registration commands.
- `THIRD_PARTY_NOTICES.md`
  - Added bundled engine notice and distribution license follow-up.

## Checks

- PASS: JSON parse smoke for `src-tauri/tauri.conf.json` and `src-tauri/sidecars/external-bin.config.json`.
- PASS: `cargo test`
- PASS: `npm run typecheck`
- PASS: `npm run build`
- PASS: `npm run tauri build -- --no-bundle`
- PASS: Release output includes:
  - `src-tauri/target/release/backend-visual-map.exe`
  - `src-tauri/target/release/engines/codebase-memory-mcp.exe`
  - `src-tauri/target/release/engines/database-memory.exe`

## Results

- The release build can include both engine executables under the app release resource directory.
- The packaging path does not require PATH lookup for the bundled engines.
- No installer, setup, MCP registration, Codex registration, Claude registration, or global config command was run.
- No DB row-data access was added.
- No DB passwords are persisted.

## Skipped Work

- Full installer smoke is intentionally deferred to Phase 71.
- Public redistribution still needs the upstream license text for both bundled engine executables before a real external release.
