# Visual Map Database Engine Contract

Version: `visual-map-database-intelligence/1`

This is the normative contract for `database-memory` when it is used as the RDB core of Visual Map. The engine describes database metadata and relationships. It never reads or persists application table rows.

## Engine goal

The DB engine must answer, with database-side evidence:

1. What database, schema, relation, column, constraint, index, view, routine, and other supported objects exist?
2. How are those objects structurally related?
3. Which exact scope, server version, adapter, and metadata privileges were inspected?
4. Is the result complete and authoritative, or did analysis fail?
5. Can a code-side reference be joined to exactly one database object without name guessing?

The engine does not resolve code calls, route handlers, ORM execution, or application behavior. Those belong to codebase-memory and the integration adapter.

## Output and storage

Each successful analysis produces one immutable snapshot in the adapter-owned cache namespace:

```text
<visual-map-cache>/<project_id>/<snapshot_id>/
  db-graph.sqlite
  db-snapshot.json
  db-capabilities.json
```

The engine writes to a temporary location, validates the complete result, closes the graph transaction, and publishes the snapshot atomically. A failed or cancelled run never replaces the last complete snapshot.

The public JSON contract and the graph cache are two views of the same canonical snapshot. The adapter must not open private engine tables or reconstruct facts from cache internals.

## Authoritative result states

The only authoritative analysis states are:

- `complete`: all requested objects and relationships were discovered, mapped, reconciled, and validated for the declared scope;
- `failed`: the engine could not prove a complete result. The failure includes a stable code, stage, redacted message, remediation, and retryability.

There is no authoritative `partial` state. A legacy or partial cache may be inspected for diagnostics but cannot be used as the current DB truth.

Example:

```json
{
  "status": "complete",
  "snapshot": {
    "contract_version": 2,
    "snapshot_id": "db-snapshot-id",
    "source_kind": "postgres",
    "connection_alias": "orders-db",
    "scope": { "catalogs": ["orders"], "schemas": ["public"] },
    "server": { "product": "PostgreSQL", "version": "16.4" },
    "adapter": { "name": "postgres", "version": "0.2.0" }
  },
  "completeness": {
    "status": "complete",
    "object_counts": [],
    "relationship_counts": [],
    "capability_checks": []
  }
}
```

## Canonical objects

Every object must have a collision-safe stable key containing its source identity, database/catalog, schema, object kind, name, and sub-object identity where applicable. Display names are not join keys.

The common object set is:

```text
Database, Schema, Table, Column, PrimaryKey, ForeignKey,
UniqueConstraint, CheckConstraint, Index, View, ViewColumn,
Trigger, Routine, MaterializedView, Sequence, RoutineParameter,
UserDefinedType, Domain, EnumValue, Synonym, ExclusionConstraint,
Event, Package, Principal, Policy, Extension
```

Each object preserves its parent key, vendor/source kind, ordered position where meaningful, nullability/type/default/generated or identity metadata where available, and vendor-specific properties without changing the common identity.

## Canonical relationships

The engine must emit typed, directed relationships rather than name-only references:

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

Vendor-specific relationships are emitted as explicit extension types with their original evidence. They are not silently dropped or renamed into a weaker common relationship.

Every relationship records `from_key`, `to_key`, direction/type, ordinal where needed, and provenance/evidence. A relationship that cannot be resolved within the declared scope causes certification failure when completeness requires it.

## Completeness proof

`completeness` must include:

- requested catalogs and schemas;
- detected product/version and adapter/version;
- discovered versus emitted counts for every modeled object category;
- discovered versus emitted counts for every modeled relationship category;
- capability checks and limitations that were actually tested;
- evidence strings that identify the catalog or metadata query family used.

`discovered == emitted` is required for every category claimed as complete. Missing privileges, unsupported catalog facts, unstable double reads, unresolved references, timeout, cancellation, or vendor-version mismatch produce `failed`, not a best-effort graph.

## Code integration boundary

The DB engine exposes only validated DB-side objects to the integration adapter. It accepts a normalized reference such as:

```text
database=orders, schema=public, kind=table, name=orders
```

The adapter may create a code-to-DB relation only when this reference resolves to exactly one object in the same declared DB snapshot. Ambiguous or absent matches remain candidates/gaps in the integrated snapshot; the DB engine does not guess.

## Metadata-only and security rules

- application rows, sample values, query results, and secrets are never read or persisted;
- connection strings and passwords are redacted from errors and artifacts;
- DDL input is metadata/schema input, never application data;
- the old snapshot remains intact if a new run fails, times out, or is cancelled;
- JSON and graph serialization is deterministic and sorted by stable key/type.

## Performance and support targets

| Area | Target gate |
| --- | --- |
| Scale | deterministic contract tests at 10k, 50k, 100k, and 1M objects with a documented interactive ceiling |
| Persistence | atomic replacement and recovery after interruption; no mixed-generation snapshot |
| Traversal | bounded list, describe, trace, impact, and diff operations with pagination/depth limits |
| RDB support | a product/version is marked supported only after its adapter contract and live/golden matrix pass |
| Failure behavior | no silent downgrade from complete to partial; stable machine-readable failure codes |
| Security | metadata-only tests prove no row/DML access and redaction tests prove no secret leakage |

These are target gates. They do not turn an unverified database/version into a supported one.

## Completion gates

The DB engine is Visual Map-ready only when:

1. complete/failed output is versioned and validated;
2. all declared-scope object and relationship counts reconcile;
3. stable keys survive re-indexing and schema ordering changes;
4. graph and JSON outputs represent the same canonical snapshot;
5. capability limitations are explicit;
6. code integration can distinguish exact, ambiguous, and missing DB matches;
7. the metadata-only, redaction, cancellation, and atomic-replacement tests pass;
8. every claimed RDB product/version has matching certification evidence.

This contract is the source of truth for DB engine changes and for the Visual Map integration adapter.
