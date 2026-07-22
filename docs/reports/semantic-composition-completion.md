# Semantic Composition Completion

Date: 2026-07-22
Branch: `feature/semantic-composition`
Product version: `0.1.0`

## Delivered

- Static SQL execution evidence now creates confirmed code-to-table
  `READS`/`WRITES` and explicit `USES_COLUMN` relationships without persisting
  query text or row data.
- API reading and DB impact views put confirmed relationships before candidates
  and keep unknowns separate.
- The fixed `관계` mode accepts 2-8 API, code, file, table, or column subjects and
  projects `전체 연결`, `호출`, `데이터`, or `영향` with bounded paths.
- Full-inventory search results can be added to the current relationship set,
  so large-project selection is not limited to the bootstrap list.
- Disconnected selected subjects remain visible and are described honestly.
- Language/framework and RDB claims are separated into engine-readable,
  product-validated, and confirmed-evidence levels.
- Evidence projections use one enriched request and do not repeat the same
  projection after a generic backend failure. Empty semantic results, enriched
  candidate variants, and selected-neighborhood paths are cached or bounded so
  repeated large-project exploration avoids unnecessary global work.

## Verification Receipt

| Check | Result |
| --- | --- |
| Frontend tests | 22 files, 111 tests passed |
| TypeScript | `tsc --noEmit` passed |
| Production frontend build | Vite build passed |
| Dead code | Knip passed |
| Rust format and lint | `cargo fmt --check`; Clippy `-D warnings` passed |
| Rust tests | 207 passed, 2 environment-gated tests ignored |
| Projection scale | 10k / 50k / 100k-item matrix passed; composition took 44 / 251 / 554 ms in the debug test profile |
| Dependency/security | 707 dependencies verified; 0 production npm vulnerabilities; metadata-only regression suite passed |
| Code engine | Contract smoke passed |
| Code matrix | Java/Spring, C#/.NET, Python/FastAPI + TypeScript pinned fixtures passed |
| DB engine | CLI, reserved identity, DDL, bulk evidence, impact, and trace contracts passed |
| Native composition | 1440 x 900 and 820 x 900 passed with one confirmed `loadOrder -> DB 조회 -> main.orders` relation |
| Internal installer | NSIS build and installer smoke passed |

The internal installer was generated at
`src-tauri/target/release/bundle/nsis/Backend Visual Map_0.1.0_x64-setup.exe`
with SHA-256
`D6253CE13DB02EE811468430582D1B65D6F1DFCE67EE136E2564689E6BDD0E9A`.
It is for local validation only.

## Honest Gaps

- Live network DB checks were skipped in this run because the PostgreSQL,
  YugabyteDB, MySQL, MariaDB, SQL Server, and Oracle test URLs were not set.
  Their exact current product boundary is recorded in `docs/product-support.md`.
- The public release gate remains closed because the pinned
  `database-memory 0.2.0` artifact is not published and is
  `releaseReady=false` in the engine manifest.
- The task protocol is ready, but no automated run is counted as evidence from
  actual junior, mid-level, or senior backend developers. The nine-participant
  release sample in `docs/usability-test-protocol.md` is still required before
  claiming usability validation.

## Decision

The semantic code-to-DB core and multi-subject composition flow are implemented
and suitable for local/internal use. Public redistribution and validated human
usability remain separate external release gates; neither is claimed complete.
