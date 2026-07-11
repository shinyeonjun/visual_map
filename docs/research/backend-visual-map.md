# Backend Visual Map Research

Status: Draft decision record
Scale: Large
Date: 2026-07-06

## Product Definition

Backend Visual Map is a local desktop app for backend developers. It takes a Git repository and one active relational database profile, indexes both with bundled graph engines, and shows a visual map of API, code, and database relationships.

The product is not a chat app, not a BI tool, not an ERD-only generator, and not a SQL execution tool. It is a visual context tool for understanding backend systems before implementing or modifying code.

## Fixed Decisions

- Product shape: desktop app.
- App stack: Tauri + React.
- Primary visual surface: React Flow / xyflow style node-edge canvas.
- Engine model: bundled executables called as child processes.
- Code engine: `codebase-memory-mcp.exe`.
- DB engine: `database-memory.exe` and optionally `database-memory-mcp.exe`.
- First target user: individual backend developer on a local machine.
- Workspace unit: one repository with multiple saved DB profiles.
- Active map scope: one active DB profile at a time.
- Default visual depth: 2-hop map.
- Code-to-DB links: candidate links, not guaranteed truth.
- Candidate links show confidence on the map.
- Clicking a candidate link shows evidence in the inspector.
- DB credentials: save non-secret profile fields; do not save passwords in v1.
- Later expansion: team sharing, PR/CI impact maps, migration risk maps, credential store.

## External Facts Checked

The following official/current references shaped the plan:

- Tauri v2 sidecars can bundle external binaries using `bundle.externalBin`.
  Source: https://v2.tauri.app/develop/sidecar/
- Tauri v2 Windows distribution supports installer builds through `tauri build`.
  Source: https://v2.tauri.app/distribute/windows-installer/
- Tauri v2 path APIs expose app data directory helpers and recommend scoped API access.
  Source: https://v2.tauri.app/reference/javascript/api/namespacepath/
- Tauri v2 file-system APIs use base directories and scoped access.
  Source: https://v2.tauri.app/plugin/file-system/
- React Flow is intended for custom node-based UIs with nodes, edges, controls, minimap, panels, and custom nodes.
  Source: https://reactflow.dev/

## Existing Engine Capabilities

### codebase-memory-mcp

Observed from local clone at `D:\codebase-memory-mcp`:

- Provides MCP tools and a CLI shape: `codebase-memory-mcp cli <tool> <json>`.
- Indexes Git repositories into a code graph.
- Stores projects, nodes, edges, and summaries in SQLite.
- Can search graph nodes.
- Can trace call paths.
- Can return architecture summaries.
- Can return code snippets.
- Has API/route concepts, HTTP calls, async calls, cross-repo intelligence, and data-flow modes.
- Has a Windows release packaging model with `codebase-memory-mcp-windows-amd64.zip`.
- MIT licensed. Bundling is allowed if copyright/license notices are preserved.

Important codebase-memory tools for this app:

- `index_repository`
- `list_projects`
- `index_status`
- `search_graph`
- `trace_path`
- `get_code_snippet`
- `get_architecture`
- `query_graph`

### rdb-memory-mcp / database-memory

Observed from local project at `D:\db_mcp`:

- Provides CLI binary `database-memory.exe`.
- Provides MCP server binary `database-memory-mcp.exe`.
- Supports release packaging through `rdb-memory-mcp-windows-amd64.zip`.
- Supports metadata-only indexing for:
  - SQLite
  - SQLite DDL files/directories
  - PostgreSQL
  - MySQL
  - SQL Server
  - Oracle
- Produces graph metadata for:
  - database
  - schema
  - table
  - column
  - primary key
  - foreign key
  - unique constraint
  - index
  - limited view/trigger/routine metadata depending on adapter
- Supports:
  - table description
  - table search
  - column search
  - impact analysis
  - relationship trace
  - schema diff
  - graph stats
  - constrained graph query
- CLI supports JSON output for:
  - `index`
  - `describe-table`
  - `find-table`
  - `find-column`
- Uses metadata-only guardrails and redacts obvious secrets in errors.

## Why Not Merge Engines

Do not fork or merge codebase-memory-mcp into rdb-memory-mcp, and do not merge rdb-memory-mcp into codebase-memory-mcp.

Reasons:

- Different implementation languages and build systems.
- Different domain ownership.
- Separate release cadences.
- Better failure isolation through child processes.
- Easier license compliance through binary bundling and notices.
- The product value is the visual orchestration layer, not a merged graph engine.

## Proposed Architecture

```text
Backend Visual Map
  src-tauri/
    commands/
      workspace
      engine
      scan
      map
    storage/
      workspace sqlite/json
    sidecars/
      codebase-memory-mcp.exe
      database-memory.exe
      database-memory-mcp.exe

  src/
    app shell
    panels
    visual canvas
    inspector
    scan progress
```

Runtime:

```text
User selects repo
  -> app calls codebase-memory-mcp.exe cli index_repository
  -> app stores code project metadata

User adds DB profile
  -> app asks for password/session connection string
  -> app calls database-memory.exe index --format json
  -> app stores DB graph cache path and profile metadata

User explores
  -> app calls codebase-memory query commands as needed
  -> app calls database-memory query commands as needed
  -> app creates candidate links
  -> React Flow renders a 2-hop visual map
```

## Workspace Storage

Use `%LOCALAPPDATA%\BackendVisualMap` on Windows. Tauri can resolve app-local data directories; the app must still create required folders explicitly.

Suggested layout:

```text
%LOCALAPPDATA%\BackendVisualMap\
  app-state.sqlite
  engines\
    codebase-memory-mcp.exe
    database-memory.exe
    database-memory-mcp.exe
  workspaces\
    <workspace-id>\
      workspace.json
      code\
        codebase-cache\
      db\
        <profile-id>\
          graph.sqlite
      atlas\
        links.sqlite
        map-cache.sqlite
        scan-events.jsonl
```

Recommended v1 storage:

- `workspace.json` for human-readable workspace metadata.
- `app-state.sqlite` for app list, recents, settings, scan history.
- DB graph caches stay as SQLite files produced by rdb-memory.
- codebase-memory cache may either stay in its default cache or be directed under workspace by environment/config if available.
- `links.sqlite` stores app-owned candidate links and evidence.

Do not store DB password in `workspace.json`.

## Workspace Model

```json
{
  "id": "stable-id",
  "name": "shop-api",
  "repo_path": "D:\\projects\\shop-api",
  "code": {
    "engine": "codebase-memory-mcp",
    "project": "shop-api",
    "last_indexed_at": "2026-07-06T00:00:00Z"
  },
  "db_profiles": [
    {
      "id": "local-postgres",
      "name": "local",
      "source": "postgres",
      "host": "localhost",
      "database": "shop",
      "username": "app",
      "cache_path": "db\\local-postgres\\graph.sqlite",
      "last_indexed_at": "2026-07-06T00:00:00Z"
    }
  ],
  "active_db_profile_id": "local-postgres"
}
```

## Visual Map Model

The UI should not render raw engine records directly. It should render an app-owned visual model:

```ts
type VisualNode = {
  id: string;
  kind:
    | "api"
    | "handler"
    | "service"
    | "repository"
    | "function"
    | "file"
    | "table"
    | "column"
    | "index"
    | "constraint"
    | "warning";
  title: string;
  subtitle?: string;
  layer: "api" | "code" | "data" | "meta";
  source: "code" | "db" | "atlas";
  ref: EngineRef;
  risk?: "low" | "medium" | "high";
};

type VisualEdge = {
  id: string;
  from: string;
  to: string;
  kind:
    | "calls"
    | "handles"
    | "imports"
    | "references"
    | "fk"
    | "index"
    | "candidate_uses"
    | "candidate_writes";
  confidence?: "high" | "medium" | "low";
  evidence?: Evidence[];
};
```

## Candidate Link Model

Candidate links are app-owned edges connecting code graph nodes to DB graph nodes. They are not source-of-truth edges in either engine.

Evidence examples:

- table name appears in a string literal
- column name appears in a string literal
- SQL-like text contains table name
- repository/DAO/class/function name contains table/domain name
- file path contains table/domain name
- API route path contains table/domain name
- code snippet near match includes CRUD-ish verb

Confidence v1:

- High:
  - SQL-like snippet contains exact table name and route/function is within the selected flow.
  - repository/DAO file name matches table and code path reaches that file.
- Medium:
  - exact table name appears in code snippet or file path.
  - table singular/plural/domain name appears in route and service names.
- Low:
  - fuzzy/domain token match only.
  - column-only match without table evidence.

Never show candidate links as confirmed unless a future explicit confirmation system is added.

## Default UI Contract

The app should use a fixed logical grid first, then make it responsive later.

Recommended first desktop frame:

```text
1440 x 900 logical target

Left rail:   320 px
Center:      flexible, min 760 px
Right rail:  360 px
Top bar:      48 px
Status bar:   28 px
```

Panel layout:

```text
┌────────────────────┬────────────────────────────┬──────────────────────┐
│ Code Source         │ Visual Map                 │ Modes / Actions       │
│ repo input          │ React Flow canvas          │ Explore               │
│ project tree        │ API -> Code -> DB          │ API Flow              │
│ API/service/file    │                            │ Table Usage           │
├────────────────────┤                            │ Column Impact         │
│ Database Source     │                            ├──────────────────────┤
│ DB profile input    │                            │ Inspector             │
│ table/column list   │                            │ evidence/details      │
└────────────────────┴────────────────────────────┴──────────────────────┘
```

## Mode Definitions

### Explore Mode

Purpose: broad navigation.

Input:

- workspace
- optional search query
- optional selected inventory item

Output:

- local 2-hop map around selected item
- code and DB inventory filters
- candidate links shown when available

### API Flow Mode

Purpose: understand one endpoint.

Input:

- API route or handler node

Output:

- route -> handler -> service/function -> repository/code candidates -> DB candidate tables/columns

### Table Usage Mode

Purpose: understand where a table is likely used.

Input:

- DB table

Output:

- table -> FK/index/columns
- candidate files/functions/routes that mention or likely use it

### Column Impact Mode

Purpose: understand schema-change impact.

Input:

- DB column

Output:

- DB impact from rdb-memory
- candidate code references
- risk summary

### Search Mode

Purpose: unified search.

Input:

- keyword

Output:

- API/code/table/column hits
- grouped results
- selecting a result creates a local map

Deferred modes:

- PR Impact Mode
- Migration Risk Mode
- Test Scope Mode
- Architecture Drift Mode
- Team Share Mode

## Security And Privacy

Hard rules:

- Everything runs locally by default.
- No source code or schema is uploaded.
- Do not read DB row data.
- Do not run arbitrary SQL for user data.
- Store DB profiles without passwords.
- Redact connection strings in logs.
- Keep sidecar command args out of persistent logs when they include secrets.
- Prefer passing secret connection strings only to child process memory for the scan operation.

Password handling v1:

- User enters password/connection string at scan time.
- App stores non-secret profile metadata only.
- App may remember that a password is required.
- Future version can use OS credential store.

## Release And Packaging

Initial app installer should bundle engines instead of downloading them on first run.

Rationale:

- better first-run UX
- no GitHub/network dependency at launch
- tested engine versions
- simpler failure handling

Bundle:

```text
BackendVisualMap-Setup.exe
  BackendVisualMap.exe
  sidecars/codebase-memory-mcp.exe
  sidecars/database-memory.exe
  sidecars/database-memory-mcp.exe
  THIRD_PARTY_NOTICES.md
  LICENSE
```

License obligations:

- Include codebase-memory MIT license text.
- Include codebase-memory copyright notice.
- Include rdb-memory MIT license.
- Include third-party notices for bundled engines.

## Risks

### Link Accuracy

Risk: code-to-DB candidate links may be wrong.

Mitigation:

- Never label them as confirmed.
- Show confidence.
- Show evidence.
- Allow hide/dismiss later.

### Large Repository Performance

Risk: indexing or map generation can be slow or visually noisy.

Mitigation:

- engine scans run in background with progress
- only render selected 2-hop subgraph
- cap nodes per map
- show "too broad, narrow selection" state

### Secret Leakage

Risk: connection strings can leak into logs or workspace files.

Mitigation:

- no password storage v1
- redaction before writing logs
- process args reviewed before persistent storage

### Engine Drift

Risk: bundled engine outputs change.

Mitigation:

- pin engine versions
- store engine version in workspace scan metadata
- normalize engine outputs into app contracts

### Tauri Sidecar Complexity

Risk: sidecar path, working directory, and bundled names differ between dev/build.

Mitigation:

- isolate engine execution behind one Rust module
- test packaged sidecar invocation in release build
- do not call sidecars directly from frontend

## Success Criteria

The v1 product is successful when:

- A user can create a workspace from a Git repo.
- A user can add one DB profile and index metadata.
- The app shows API/code/DB inventory.
- Selecting an API renders a 2-hop visual map.
- Selecting a table renders related DB objects and candidate code/API links.
- Selecting a column renders DB impact and candidate code references.
- Candidate links show confidence and evidence.
- No DB row data is read.
- Passwords are not stored.
- Windows installer includes both bundled engines.

