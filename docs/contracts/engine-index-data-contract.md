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

The logical output is stored as:

```text
<cache>/<project_id>/<snapshot_id>/
  code-graph.sqlite
  code-snapshot.json
  code-gaps.json
  code-capabilities.json
  db-graph.sqlite
  db-snapshot.json
  db-capabilities.json
  integrated-snapshot.json
```

`graph.sqlite` is the source of truth for nodes and edges. JSON manifests hold
generation status, capability limits, and gaps. `integrated-snapshot.json`
contains only joins that were made from the two published snapshots; it is not
allowed to mutate either engine's graph.

All graph facts must be deterministically ordered by stable key, edge type,
source location, and target stable key. A new index creates a new snapshot
directory rather than mutating a snapshot currently being read.

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
