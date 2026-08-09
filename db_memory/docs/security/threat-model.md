# Database Memory threat model

## Security boundary

The engine reads database schema metadata and local DDL/SQLite sources on behalf
of the desktop app. It must not read application table rows, execute migrations,
or expose credentials in persisted snapshots, diagnostics, or logs.

Protected assets are database credentials, local source files, schema metadata,
the last certified snapshot, and the host account's filesystem authority.

## Trust assumptions

- Repository and DDL text are untrusted input.
- Connection strings and environment variables are secrets.
- The desktop app is the only product caller; CLI arguments remain untrusted.
- A database account can see only what its granted catalog privileges allow.
- The local OS account and its protected application-data directory are trusted.

## Required controls

- Adapters query catalogs, schema objects, PRAGMA metadata, or isolated DDL
  metadata only; row access is prohibited by contract and regression tests.
- SQLite files are opened read-only. DDL is evaluated in an isolated in-memory
  database with attachment, extension loading, virtual tables, row statements,
  oversized input, and unbounded execution denied.
- New generations publish transactionally only after completeness certification.
- Missing privilege, unsupported metadata, unresolved dependency, catalog drift,
  timeout, or cancellation produces a structured failure rather than invented
  graph data.
- Connection strings are accepted at runtime and excluded from graph snapshots.
  Profile-scoped environment variables are the preferred input path.
- Errors are redacted before crossing the engine boundary.
- Traversals and result pages are bounded and disclose truncation.
- Release artifacts carry hashes and are verified before packaging.

## Residual risks

- A live catalog can change during introspection. Certification detects known
  count mismatches but cannot make an external database globally immutable.
- Database drivers and native client libraries retain their upstream attack
  surface; tracked exceptions live in `dependency-exceptions.md`.
- Metadata itself can be sensitive. Local application data and exported
  diagnostics must retain OS-level access controls.
- Supplying a connection string on a command line can expose it to shell history
  or process inspection; use the documented environment-variable path.

Any future network or agent-facing database interface requires a separate
threat-model revision before implementation.
