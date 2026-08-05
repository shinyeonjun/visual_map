# Engine Index Data Contract

Version: `visual-map-index-data/1`

This is the canonical inventory of data that the code and database cores must
persist when they index a project. It is deliberately narrower than a raw AST
or vendor catalog dump: every stored fact must help Visual Map answer project
understanding, execution-path, or change-impact questions.

The contract defines data, not screen layout. The adapter may project these
facts into focused views, but it may not invent facts that are absent here.

## 1. Shared snapshot envelope

Both engines produce a snapshot with the following identity and lifecycle data:

```json
{
  "contract_version": "visual-map-index-data/1",
  "project_id": "stable-project-id",
  "snapshot_id": "stable-snapshot-id",
  "source_root": "D:/repos/orders-backend",
  "branch": "main",
  "commit_sha": "abc123",
  "indexed_at": "2026-07-26T00:00:00Z",
  "engine": {
    "name": "code-memory-language",
    "version": "0.1.0"
  },
  "analysis": {
    "status": "complete",
    "scope": "full",
    "items_seen": 120,
    "items_indexed": 118,
    "items_failed": 2,
    "gaps": 7
  }
}
```

Rules:

- `project_id` identifies the repository, while `snapshot_id` identifies one
  immutable analysis generation.
- `status` is `complete`, `partial`, `failed`, `stale`, or `unsupported` for
  the code side; the DB core uses its stricter `complete`/`failed` authority
  contract.
- A failed generation never replaces the last published complete generation.
- Unknown values are `null`; empty strings and fake defaults are forbidden.
- Paths are repository-relative inside nodes and use `/` separators.

## 2. Storage layers

The logical contract above is independent of its physical encoding. The desktop
app currently stores it below the workspace so deleting a workspace also
deletes its engine state:

```text
%LOCALAPPDATA%/VisualMap/workspaces/<workspace-id>/
  atlas/
    inventory-snapshot.sqlite
    inventory-snapshot.backup.sqlite        # present after rotation
  engines/
    codebase-memory/0.1.0/contract-1/cache/
      compat-projects/<project>/
        current.json
        previous.json                       # present after the second publish
        generations/<generation-id>/
          receipt.json
          code-graph.sqlite
      runtime/
        <content-addressed provider caches>.json.gz
    database-memory/0.2.0/contract-2/profiles/<profile-id>/
      <database graph store>
```

### 2.1 Code generation store

`current.json` and `previous.json` use
`code-memory.generation-receipt.v1`. A receipt names one immutable generation,
its canonical database path, status, creation time, project root, and exact
inventory/CALLS/HANDLES/architecture counts. The database metadata schema is
`code-memory.graph-store.v3`.

`code-graph.sqlite` keeps searchable identity columns in ordinary indexed
tables and stores lossless payloads as GZip-compressed chunks of at most 512
records. The thin tables cover inventory names and paths, relationship
endpoints, and architecture node/edge identities. They are indexes, not a
second semantic representation.

Publishing is complete-or-failed:

1. write a private `.staging-<generation-id>` directory;
2. commit and sync `code-graph.sqlite`;
3. rename the staging directory to `generations/<generation-id>`;
4. atomically rotate `current.json` to `previous.json` and publish the new
   current receipt;
5. retain only the current and previous complete generations.

A failed or interrupted generation never replaces `current.json`. Readers open
the database read-only and verify the receipt schema, status, canonical path,
workspace boundary, and stored counts before accepting it.

### 2.2 Integrated Tauri snapshot store

`inventory-snapshot.sqlite` uses `visual-map.snapshot-store.v1`. The snapshot
header and lossless item/link/architecture payloads are GZip-compressed in
chunks of at most 512 records. `item_index`, `link_index`,
`architecture_node_index`, and `architecture_edge_index` contain only bounded,
secret-redacted lookup fields. Search queries these indexes and decompresses
only result chunks; views that need the full project may still materialize the
complete snapshot.

Save writes a temporary SQLite file, validates it, rotates a readable current
file to `inventory-snapshot.backup.sqlite`, and renames the new file into place.
The database is immutable after publication, so it does not require a WAL or
background database service.

### 2.3 Compatibility and ordering

- Legacy code project files (`language-index.json`, `architecture.json`, and
  `collection-report.json`) are migrated on first read. They are deleted only
  after the new SQLite generation reopens successfully.
- Legacy app snapshots (`inventory-snapshot.json.zip` and
  `inventory-snapshot.json`) remain readable. The next successful save writes
  SQLite, preserves one readable SQLite backup, then removes the legacy files.
- Existing public interchange commands may still emit JSON. Those files are
  export artifacts, not the desktop app's persistent hot path.
- All graph facts remain deterministically ordered by stable key, relationship
  type, source location, and target stable key before chunking. Compression and
  chunk boundaries must not change logical output.

A storage schema change is internal and does not by itself change
`visual-map-index-data/1`. The logical contract version changes only when
consumer-visible fields or semantics change.

## 3. Common graph record shape

Every node has this logical record, whether fields are SQLite columns or JSON
properties:

```json
{
  "id": 42,
  "stable_key": "code:file:app/orders/service.py",
  "label": "File",
  "name": "service.py",
  "qualified_name": "app.orders.service",
  "file_path": "app/orders/service.py",
  "start_line": 1,
  "end_line": 120,
  "engine": "code",
  "language": "python",
  "module": "app.orders.service",
  "is_test": false,
  "is_generated": false,
  "properties": {}
}
```

Required node rules:

- `id` is local to one graph and must never be used to join code and DB data.
- `stable_key` is deterministic and is the only cross-snapshot join identity.
- `label` is semantic and comes from the engine, not from the UI.
- Source locations are exact 1-based lines or `null`.
- `properties` preserves useful language/vendor facts without replacing common
  fields.

Every edge has this logical record:

```json
{
  "id": 91,
  "type": "CALLS",
  "source_key": "code:symbol:app.orders.OrderHandler.complete",
  "target_key": "code:symbol:app.orders.OrderService.complete",
  "truth_class": "confirmed",
  "evidence_type": "AST_EXPLICIT",
  "strategy": "import_binding",
  "confidence": 0.98,
  "source": {
    "file_path": "app/orders/handler.py",
    "start_line": 18,
    "end_line": 18
  },
  "evidence": "OrderHandler.complete calls OrderService.complete",
  "analysis_scope": "full",
  "properties": {}
}
```

Every edge must have a source location when it is `confirmed`. Candidates must
also have `candidate_reason` and, when known, `candidate_count`. `unknown` is a
gap state, not a confirmed edge.

## 4. Code-core data

### 4.1 Required code node types

The code core stores these nodes when present in the source:

```text
Project, Snapshot, AnalysisRun,
File, Module, Package,
EntryPoint, Function, Method, Constructor,
Class, Interface, Struct, Trait, Enum,
Field, Property,
Handler, Service, RepositoryOperation, Query,
ExternalService, Event, Test, Config, DbReference
```

Minimum symbol properties when applicable:

```json
{
  "signature": "complete(order_id: UUID) -> Order",
  "visibility": "public",
  "async": true,
  "framework": "fastapi",
  "decorators": ["router.get"],
  "parameters": [],
  "return_type": "Order"
}
```

The engine does not create a `Handler`, `Service`, `Table`, or `Column` merely
to fill a visual lane. Semantic nodes exist only when source or framework
evidence supports them.

### 4.2 Required code relationship types

```text
CONTAINS_FILE
DEFINES
IMPORTS
EXPORTS
REEXPORTS
CALLS
AWAIT_CALLS
CONSTRUCTOR_CALLS
HANDLES
INHERITS
IMPLEMENTS
OVERRIDES
TRAIT_USES
DISPATCHES_TO
RETURNS_TYPE
ACCEPTS_TYPE
USES_TYPE
HTTP_CALLS
ASYNC_CALLS
TESTS
TESTS_FILE
```

Relationship meaning is fixed:

- `CALLS` means a statically supported execution call, not name similarity.
- `HANDLES` means a route/event/job registration reaches a handler.
- `IMPORTS`, `EXPORTS`, and `REEXPORTS` preserve local/imported names and
  aliases in `properties`.
- `DISPATCHES_TO` may have multiple candidates; it must not choose one at
  random.
- dynamic import, reflection, DI, and opaque runtime dispatch become gaps or
  candidates with reasons.

### 4.3 Entry point data

Every detected execution boundary is normalized as `EntryPoint`:

```text
HTTP_ROUTE, RPC_ENDPOINT, EVENT_CONSUMER, QUEUE_HANDLER,
SCHEDULED_JOB, CLI_COMMAND, WEBHOOK
```

Required properties are `kind`, the route method/path or event/topic name,
framework, handler key when resolved, and registration source location.

### 4.4 Code-to-database reference data

The code core stores the evidence needed for DB joining, not a confirmed DB
relationship:

```json
{
  "label": "Query",
  "properties": {
    "operation": "write",
    "table_reference": "orders",
    "column_references": ["status"],
    "source_file": "app/orders/repository.py",
    "start_line": 31,
    "sql_fingerprint": "sha256:...",
    "parser": "static_sql",
    "orm_framework": null
  }
}
```

Required code-side boundary:

```text
RepositoryOperation
  -[EXECUTES_QUERY]-> Query
  -[REFERENCES_DB]-> DbReference
```

`DbReference` remains `candidate` until the DB snapshot resolves it to exactly
one database object. The code core must never emit a confirmed `READS`,
`WRITES`, `USES_COLUMN`, or `MAPS_TO` edge by string matching alone.

Local variable data flow must be distinct from database access:

```text
READS_VALUE / WRITES_VALUE  local variable flow
READS / WRITES              validated database access in integrated snapshot
```

## 5. Database-core data

### 5.1 Required DB object types

The DB core stores every object supported by the certified adapter and its
declared scope:

```text
Database, Schema, Table, Column,
PrimaryKey, ForeignKey, UniqueConstraint, CheckConstraint,
Index, View, ViewColumn, Trigger, Routine, MaterializedView,
Sequence, RoutineParameter, UserDefinedType, Domain, EnumValue,
Synonym, ExclusionConstraint, Event, Package, Principal, Policy,
Extension
```

Minimum DB object fields:

```json
{
  "stable_key": "db:postgres:orders:public:table:orders",
  "kind": "Table",
  "name": "orders",
  "parent_key": "db:postgres:orders:public",
  "database": "orders",
  "schema": "public",
  "properties": {
    "vendor": "postgres",
    "comment": null
  }
}
```

Columns additionally preserve ordinal position, data type, nullability,
default/generated/identity information, and exact parent table key. Keys and
indexes preserve ordered columns, expressions, uniqueness, and included columns
where the adapter can prove them.

### 5.2 Required DB relationship types

```text
DATABASE_HAS_SCHEMA
SCHEMA_HAS_TABLE
TABLE_HAS_COLUMN
TABLE_HAS_CONSTRAINT
CONSTRAINT_COLUMN
FOREIGN_KEY_COLUMN_PAIR
TABLE_HAS_INDEX
INDEX_COLUMN
SCHEMA_HAS_VIEW
VIEW_DEPENDENCY
TRIGGER_TARGET
TRIGGER_ROUTINE
SCHEMA_HAS_ROUTINE
ROUTINE_DEPENDENCY
METADATA_PARENT
METADATA_RELATIONSHIP
```

Every relationship stores `from_key`, `to_key`, type, ordinal where meaningful,
vendor/source provenance, and completeness evidence. If the adapter claims a
scope is complete, discovered and emitted counts must reconcile for every
declared object and relationship category.

### 5.3 DB proof and capability data

The DB snapshot stores:

- product and server version;
- adapter name and version;
- catalog/schema scope;
- discovered versus emitted counts;
- capability checks and limitations;
- metadata query evidence;
- failure code/stage/remediation when analysis fails.

The DB engine does not store row samples, query results, credentials, or
unredacted connection strings.

## 6. Integrated data

The integration layer joins only published snapshots using stable keys and
validated normalized references. It may add these edges:

```text
EXECUTES_QUERY
READS
WRITES
USES_COLUMN
MAPS_TO
```

Promotion rules:

| Situation | Stored result |
| --- | --- |
| One exact DB object match | confirmed integrated edge |
| Multiple DB objects match | candidate + ambiguity gap |
| No DB object match | unresolved gap |
| DB snapshot missing/failed | code-side `DbReference` remains candidate |
| SQL/ORM operation not statically understood | dynamic_sql or unsupported_syntax gap |

The integrated snapshot also stores the source code edge, DB edge, and join
evidence so the UI can explain both sides of a relation.

## 7. Must not be stored or fabricated

- application row data or sampled values;
- passwords, tokens, or raw connection strings;
- fake counts, placeholder nodes, or fallback project names;
- confirmed relationships based only on matching names;
- a single chosen target when dispatch is ambiguous;
- a database table/column that was not present in a validated DB snapshot;
- UI-only grouping, card positions, colors, or labels in the engine graph.

## 8. Completion gates

The two cores are ready for Visual Map integration only when:

1. every required record has a stable key and snapshot identity;
2. every confirmed edge has evidence and a source/proof location;
3. full and incremental indexes do not leave stale records;
4. sequential and parallel indexing produce equivalent logical output;
5. gaps and capability limits are persisted, not hidden;
6. code-to-DB joins distinguish exact, ambiguous, missing, and unsupported;
7. DB complete/failed authority and metadata-only tests pass;
8. no UI or adapter fallback invents missing engine data.

This document is the data checklist for codebase-memory, database-memory, the
integration adapter, and the fixture tests.

## 9. Golden contract check

The minimum serialized shape is captured in
`code_memory/tests/fixtures/engine-index-contract.json` and checked by
`index_output_matches_contract_golden_shape` in the Rust test suite. The
fixture intentionally validates field names and required sections, not a
large project snapshot.

When the contract changes, update the fixture and the test assertion together,
then run:

```powershell
cargo test --manifest-path code_memory/rust/Cargo.toml index_output_matches_contract_golden_shape
cargo test --locked --manifest-path src-tauri/Cargo.toml visual_map_contract_serializes_structured_fields
```
