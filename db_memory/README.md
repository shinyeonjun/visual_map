# Database Memory engine

Database Memory is the local, metadata-only relational database analysis engine
used by the desktop product. It converts database catalogs, SQLite files, and
DDL into a versioned schema graph without querying application rows.

```text
catalog / SQLite / DDL -> certified metadata snapshot -> local graph -> desktop map
```

It is not a SQL console, row-data assistant, migration runner, or deployment
tool. The only supported application integration is the `database-memory` CLI;
the desktop app owns AI access and user interaction.

## Product boundary

- Metadata only: `row_data_access: false` is part of the executable contract.
- Fail closed: analysis publishes either a certified `complete` snapshot or a
  structured `failed` result. It never publishes a partial snapshot as truth.
- Local first: graph caches stay on the user's machine.
- Bounded reads: object pages and relationship details report clamping and
  truncation.
- Stable identity: every object and relationship has a deterministic key.
- Transactional replacement: failed re-indexing leaves the last complete
  generation intact.

The engine currently contains adapters for PostgreSQL/YugabyteDB,
MySQL/MariaDB, SQLite and SQLite-compatible DDL, SQL Server, and Oracle. An
optional ODBC build is restricted to the certified SQL Server bridge. Exact
versions, scopes, and capabilities come from the runtime contract rather than
marketing copy:

```powershell
database-memory contract --format json
```

## Build and test

```powershell
cargo build --release
cargo test --locked --workspace
```

The resulting binary is `target/release/database-memory.exe` on Windows and
`target/release/database-memory` on macOS/Linux. The parent desktop repository
packages this binary as an internal engine.

## Example

```powershell
database-memory index --source ddl-sqlite --path examples/sample-schema.sql --alias shop --cache-path examples/shop-cache.sqlite
database-memory list-objects ddl-sqlite:shop --kind table --format json --cache-path examples/shop-cache.sqlite
database-memory find-objects ddl-sqlite:shop orders --kind table --format json --cache-path examples/shop-cache.sqlite
database-memory describe-object ddl-sqlite:shop v2:sqlite:shop:main:main:table:orders --relationship-limit 100 --format json --cache-path examples/shop-cache.sqlite
```

The CLI deliberately exposes only snapshot and generic object reads. Typed
trace, impact, and schema-diff primitives remain in the Rust core for the
desktop Fact Graph adapter; they are not separate end-user command surfaces.

For live databases, prefer a profile plus a runtime environment variable so a
connection string does not enter shell history:

```toml
[warehouse]
source = "postgres"
```

```powershell
$env:DATABASE_MEMORY_WAREHOUSE_CONNECTION_STRING = "postgresql://user:password@host/database"
database-memory index --alias warehouse --cache-path .database-memory/warehouse.sqlite
```

## Authoritative references

- [Install and platform requirements](docs/install.md)
- [Testing and live certification](docs/testing.md)
- [Desktop output contract](docs/contracts/codebase-workspace-database-output-contract.md)
- [Threat model](docs/security/threat-model.md)
- [Dependency exceptions](docs/security/dependency-exceptions.md)
