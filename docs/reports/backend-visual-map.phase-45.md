# Backend Visual Map Phase 45

Date: 2026-07-06

## Changed Files

- `src-tauri/src/engine.rs`
- `src-tauri/src/engine_tests.rs`
- `src-tauri/src/workspace/code.rs`
- `src-tauri/src/workspace/db.rs`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Added centralized sidecar argument construction and validation.
- Routed code and DB engine calls through one engine command boundary.
- Kept timeout handling and stdout/stderr redaction in the shared engine runner.
- Reused the shared version/availability path for per-engine version checks.
- Added guards that reject installer, setup, MCP registration, and script-style sidecar arguments.
- Added a missing-engine smoke test that fails before process spawn.

## Checks

- `cargo test` passed: 44 tests.
- `npm run typecheck` passed.
- `npm run build` passed.
- `cargo test run_engine_command_rejects_missing_engine_before_spawn` passed.

## Skipped Work

- Did not implement Phase 46 GitHub clone flow.
- Did not run installer scripts or auto-register MCP config.
- Did not add DB row-data access.
- Did not persist DB passwords.
- Did not render raw full graphs directly.

## Risks

- Sidecar argument validation is intentionally narrow; future legitimate sidecar commands that resemble setup or registration actions should be added deliberately at the centralized boundary.
