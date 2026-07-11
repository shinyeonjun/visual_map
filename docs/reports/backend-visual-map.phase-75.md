# Backend Visual Map Phase 75 Report

## Decision

**hold**

The product is close to a release candidate for local Windows demo/use, but it is not ready for public release. Core local code + SQLite DDL metadata flows pass, installer smoke passes, and screenshot QA passes. Release should stay on hold until the blockers below are closed or the release scope is explicitly cut.

## Reviewed Inputs

- Reviewed phase reports 41-74: all required report files are present under `docs/reports`.
- Reviewed product-completion plan status for phases 64-74: complete.
- Reviewed final installer artifact:
  - `src-tauri/target/release/bundle/nsis/Backend Visual Map_0.1.0_x64-setup.exe`

## Final Checks

- PASS: `npm run typecheck`
- PASS: `npm run build`
- PASS: `cargo test` - 55 tests passed.
- PASS: Real code engine smoke:
  - Engine: `src-tauri/engines/codebase-memory-mcp.exe`
  - Query: `api route endpoint`
  - Result included `/api/v1/sessions` route candidates.
- PASS: Real DB engine SQLite DDL smoke:
  - Engine: `src-tauri/engines/database-memory.exe`
  - DDL: `D:\meeting-overlay-assistant\server\app\infrastructure\persistence\postgresql\drawsql\030_drawsql_schema.sql`
  - Indexed 18 tables, 174 columns, 61 constraints, 24 indexes.
  - `sessions` returned 12 columns.
- PASS: Real PostgreSQL smoke.
  - Container: `caps-postgresql-dev`, `pgvector/pgvector:pg16`, `127.0.0.1:55432`.
  - Command: `scripts/smoke-rdb-productization.ps1 -DatabaseMemory src-tauri/engines/database-memory.exe`
  - Environment: `BACKEND_MAP_TEST_POSTGRES_URL=<redacted>`.
  - Result: `PASS PostgreSQL: metadata index completed.`
- PASS: Secret persistence spot check on the Phase 71 final smoke workspace.
  - No connection string, URL password, `Password=`, `Pwd=`, token assignment, secret assignment, or `passwordStored: true` pattern was found.

## Release Blockers

- Public redistribution license notices are incomplete.
  - `THIRD_PARTY_NOTICES.md` still says upstream engine license text and copyright notices must be added before public distribution.

## Non-Blocking Known Issues

- Tauri build warns that identifier `com.backendvisualmap.app` ends with `.app`; this is a macOS naming warning, not a Windows installer blocker.
- Full clean-profile install testing was not available. Phase 71 used a temporary install directory and verified no Codex/Claude global config files were created.
- Current `database-memory find-column` line output does not include column types, so some UI cells show `타입 ?`.
- Engine version metadata is partial.
  - Bundled `codebase-memory-mcp.exe --version` returns `codebase-memory-mcp 0.8.1`, and the app metadata now matches `0.8.1`.
  - `database-memory.exe --version` returns `unknown command '--version'`, so app metadata records it as `unknown`.

## Product Criteria

- PASS: Workbench and Atlas IA are clearer after Phases 64-65.
- PASS: Real `meeting-overlay-assistant` product smoke was attempted and passed for local code + SQLite DDL metadata.
- PASS: RDB smoke matrix is honest with pass/skip/fail states; PostgreSQL now has a live pass.
- PASS: Security/privacy audit checked persisted files, logs, row-data access, password persistence, and MCP auto-registration.
- PASS: Sidecar packaging and installer smoke do not depend on PATH.
- PASS: Screenshot QA checked blank/white-screen states, clipping, and focused modes.
- PASS: Demo story is repeatable in 3 minutes.

## Guardrails Confirmed

- No DB row-data access was added.
- No DB passwords were persisted.
- No MCP server was auto-registered into Codex, Claude, or another AI tool.
- `codebase-memory-mcp.exe` and `database-memory.exe` are treated as internal sidecar engines only.
- Raw full graph rendering remains blocked; the UI uses grouped/focused maps.
- Code-to-DB links remain candidates unless direct evidence proves them.
- UI remains Korean-first.

## Scope Recommendation

To ship immediately before public redistribution notices are complete, cut public scope to:

- Windows local desktop app.
- Local code indexing.
- SQLite, SQLite DDL, and PostgreSQL metadata.
- Demo/non-production use with bundled engines.

To ship publicly, keep the release on hold until the remaining blocker is closed.
