# DB memory baseline — 2026-08-03

## Scope

This is a deterministic local baseline. No live database credentials or
external database services were configured in this run, so live adapter tests
remain explicitly ignored. The database engine test run completed with 11 CLI
unit tests, 1 CLI/MCP parity test, 155 core tests, and 14 MCP tests passing;
27 core tests were correctly ignored behind their named live-database
environment variables.

## Object and relationship coverage

The adapter contract exposes metadata-only objects and typed relationships.
The following matrix is the product boundary currently implemented and
documented in `docs/product-support.md` and
`db_memory/docs/contracts/visual-map-database-output-contract.md`.

| Source | Tables / columns | PK / FK / unique / check | Indexes | Views / view columns | Triggers | Routines / parameters | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SQLite | yes | yes | yes | yes, DDL/runtime dependent | yes | bounded/adapter dependent | local contract + smoke |
| SQLite DDL | yes | yes | yes | yes when represented in DDL | yes | bounded DDL grammar | deterministic DDL tests |
| PostgreSQL | yes | yes | yes | yes | yes | yes | certified adapter contract; live matrix env-gated |
| YugabyteDB YSQL | yes | yes | yes | yes | yes | yes | certified product/version boundary; live env-gated |
| MySQL | yes | yes | yes | yes | yes | yes | certified version matrix; live env-gated |
| MariaDB | yes | yes | yes | yes | yes | yes | certified version matrix; live env-gated |
| SQL Server | yes | yes | yes | yes | yes | yes | certified product/version boundary; live env-gated |
| Oracle | yes | yes | yes | yes/materialized views | yes | routines, packages, parameters | certified product/version boundary; live env-gated |

The graph contract also carries `VIEW_DEPENDENCY`, `ROUTINE_DEPENDENCY`,
`TRIGGER_TARGET`, and `TRIGGER_ROUTINE` relationships. The Visual Map adapter
keeps these as database-side evidence; it does not turn ORM-generated SQL into
confirmed code edges.

## Static SQL measurement

The parser's checked-in semantic-link corpus contains 13 representative
execution forms: 7 are intentionally confirmed by the bounded grammar and 6
are intentionally rejected as dynamic, ambiguous, unsupported, or non-executed
forms. That is a 53.8% confirmation rate for this adversarial contract corpus,
not a claim about all SQL in a repository. ORM-generated SQL, interpolated or
concatenated SQL, CTEs, multi-statement literals, unsupported dialect syntax,
and ambiguous names remain candidates/unknowns.

The number is useful as a regression baseline: widening the parser may improve
coverage, but must preserve the negative cases and must not invent a table or
column edge.

## Known limitation

View and routine dependencies are collected by the DB engine when the adapter
can prove them from catalog metadata. Static application SQL that reaches a
view or routine indirectly is not upgraded by name matching. If the source DB
adapter reports a capability gap or the live catalog privilege proof is
incomplete, the integrated snapshot retains an explicit gap instead of
claiming “no dependency”.

## Reproduction

```powershell
cargo test --locked --manifest-path db_memory/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml semantic_links
```

Live certification must be run separately with the specific
`DATABASE_MEMORY_TEST_*_URL` variables described in `db_memory/docs/testing.md`.
