# Product Support Boundary

Status: Current product contract
Last updated: 2026-07-24

Backend Visual Map separates three different claims:

- **Engine-readable**: the pinned engine can parse or index the input.
- **Product-validated**: the desktop flow was exercised against a pinned real
  project or database version.
- **Confirmed relationship**: the product has direct evidence for this exact
  edge. Engine support alone never upgrades an edge to confirmed.

## Code Support

The bundled code engine is pinned to `codebase-memory 0.9.0`. Its upstream
parser covers many languages, but this product does not claim equal API,
handler, and call-chain quality for every parser grammar.

| Product validation set | Pinned fixture | Validated product fields |
| --- | --- | --- |
| Java / Spring | `spring-projects/spring-petclinic@51045d1` | routes, symbols, source locations, scored calls |
| C# / .NET FastEndpoints | `ardalis/CleanArchitecture@a064d0b` | static `Configure` routes, exact `ExecuteAsync` / `HandleAsync` handlers, symbols, source locations, scored calls |
| Python / FastAPI + TypeScript | `fastapi/full-stack-fastapi-template@4cd0d9e` | routes, symbols, source locations, scored calls |

Other engine-readable languages remain available for inventory exploration,
but their framework route extraction and end-to-end call quality are not
product-certified. A missing route, handler, or call edge is shown as unknown;
it is never inferred from a familiar name alone.
For FastEndpoints, the product accepts only one static HTTP registration in an
indexed `Configure` method on an `Endpoint` type, one exact execution method in
the same type, and either a literal route or an exact indexed `const string`.
Dynamic, ambiguous, and non-endpoint `Configure` methods fail closed.

### Code Relationship Rules

- `HANDLES` and `CALLS` keep the confidence emitted by the code engine.
- CALLS at 85% or above may enter a confirmed path; 70-84% stays candidate;
  lower or unscored output stays unknown.
- A stale source snapshot must be read again before it can answer a focused
  request.
- Files outside the registered repository are never inspected.

### Confirmed Code-to-DB Rules

`READS`, `WRITES`, `USES_COLUMN`, and the `EXECUTES_QUERY` evidence label are
created only when all of these are true:

1. the source file resolves inside the registered repository;
2. the inspected file is at most 2 MiB and the code range is at most 240 lines;
3. a static SQL literal is the complete first argument to a recognized
   execution call, or is assigned to a local variable used as that complete
   first argument;
4. the operation is `SELECT`, `INSERT`, `UPDATE`, `DELETE`, or `MERGE`;
5. the referenced table resolves to exactly one indexed DB table;
6. a column edge is emitted only for an explicit indexed column identifier.

Recognized method calls must use a bounded DB receiver name such as
`connection`, `cursor`, `jdbcTemplate`, `entityManager`, `session`, `client`,
`pool`, `sequelize`, `prisma`, `knex`, or `sql`. MyBatis SQL annotations and
`sqlx::query` are handled as explicit framework forms. Generic receivers such
as `logger.raw` never become confirmed evidence.

Composite statements separate their targets: `INSERT ... SELECT`,
`UPDATE ... FROM`, `DELETE ... USING`, and `MERGE ... USING` write the target
and read statically named source tables. Qualified join columns belong only to
their matching alias/table. Unqualified columns are confirmed only when one
resolved table owns that indexed column.

The evidence records the source file, exact SQL-literal line, operation, and
resolved DB object. Query text and row data are not persisted.

The following remain candidate or unknown:

- interpolated, concatenated, generated, or otherwise dynamic SQL;
- CTEs, multi-statement literals, comma joins, table functions, and nested
  query forms whose ownership cannot be proven by the bounded parser;
- dialect-specific projection clauses outside the bounded grammar, including
  SQL Server `TOP` and PostgreSQL `DISTINCT ON`;
- ORM-generated queries without an explicit static SQL literal;
- a SQL-looking help string that is not passed to the execution call;
- a static variable that is reassigned before execution;
- an unqualified table name that exists in more than one indexed schema;
- an unqualified column shared by multiple tables in the same query;
- SQL comments and temporary-table names that merely contain a real table name;
- a framework execution API outside the recognized call set;
- parameter names and string values that merely match a column name.

`USES_COLUMN` proves that the SQL names the column. It does not claim the
runtime value, row contents, transaction outcome, or production execution.

## Database Support

The bundled DB adapter is pinned to `database-memory 0.2.0 / contract 2`.
Every source is metadata-only.

| Source | Adapter-certified boundary | Desktop product evidence |
| --- | --- | --- |
| SQLite | bundled runtime, `main` catalog | contract and native product smoke |
| SQLite DDL | SQLite-compatible schema DDL | contract, evidence, stale-source smoke |
| PostgreSQL | 14-18, one database and selected schemas | PostgreSQL 16 live product smoke |
| YugabyteDB YSQL | `15.12-YB-2025.2.3.2-b0` | product path implemented; no current live desktop receipt |
| MySQL | 8.0, 8.4, 9.7, one database | MySQL 8.4 live product smoke |
| MariaDB | 10.11, 11.4, 11.8, 12.3 | product path implemented; no current live desktop receipt |
| SQL Server | 2017, 2019, 2022, 2025 Database Engine | SQL Server 2022 live product smoke |
| Oracle | Oracle AI Database 26ai Free `23.26.2.0.0` | Oracle Free 23.26.2 via Instant Client 19.30 |
| Generic ODBC | not exposed as a generic product source | SQL Server bridge only; other products are not claimed |
| DB2 | unsupported | no adapter and no product path |

Azure SQL variants, YCQL, compatible-but-different database products, and
versions outside the certified adapter ranges fail closed or remain
unsupported. Oracle requires a 64-bit Oracle Client 11.2 or newer.

## Composition Support

The `관계` view accepts 2-8 existing API, code, file, table, or column items.
The bounded left list keeps common items nearby; the global inventory search
can add items outside that bootstrap list without leaving relationship mode.
It offers four fixed projections:

- **전체 연결**: shortest confirmed connecting paths, then candidate fallback;
- **호출**: confirmed handler/call relationships only;
- **데이터**: API/code-to-DB, containment, DB dependency, and candidate data use;
- **영향**: connecting paths plus one bounded neighboring hop.

Paths are capped at 8 hops, 20,000 searched nodes per selected pair, 40 visible
nodes, and 80 visible edges. Disconnected selected items stay visible and are
reported as not connected rather than being removed.

Before visual edges or name-based candidates are created, the projector limits
work to the selected subjects' confirmed 8-hop neighborhood. That scope is
bounded to 20,000 nodes and 80,000 graph edges; reaching either cap is reported
as partial rather than silently treated as a complete absence.
The source index scan is additionally capped at 100,000 inventory items and
200,000 stored links. Candidate bridges are capped separately and may connect
two selected code components through the same unselected DB object.

## Distribution Status

- Source and local/internal Windows builds are supported.
- `codebase-memory 0.9.0` is release-ready in the engine manifest.
- `database-memory 0.2.0` remains `releaseReady=false` until its public release
  artifact and checksum are published and reverified.
- Therefore an official public installer is intentionally blocked. This does
  not block local development or internal use.

## Acceptance Rule

A capability moves from engine-readable to product-validated only after a
pinned fixture or live version passes the product smoke and its evidence is
recorded. It moves to confirmed relationship only when the current snapshot
contains direct, inspectable evidence for that exact edge.
