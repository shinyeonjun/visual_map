# Backend Visual Map Meeting Overlay Smoke

Date: 2026-07-06

Target:

- Repository: `D:\meeting-overlay-assistant`
- Database: PostgreSQL from `deploy/server/docker-compose.infrastructure.yml`
- Local connection: `postgresql://caps:***@127.0.0.1:55432/caps`

Setup:

- Started `caps-postgresql-dev` with Docker Compose.
- Applied `server/app/infrastructure/persistence/postgresql/020_runtime_with_pgvector_schema.sql`.
- Verified metadata only: 20 public tables, extensions `citext`, `plpgsql`, and `vector`.

Database engine smoke:

- Engine: `%APPDATA%\com.backendvisualmap.app\engines\database-memory.exe`
- Command: `index --source postgres --connection-string <redacted> --alias meeting-overlay`
- Result: pass.
- Indexed: 20 tables, 221 columns, 73 constraints, 74 indexes.
- Query checks:
  - `find-table session` returned `auth_sessions`, `session_participants`, `session_post_processing_jobs`, and `sessions`.
  - `describe-table sessions` returned columns, primary key, 9 inbound FKs, 4 outbound FKs, and 7 indexes.
  - `find-column session_id` returned 9 referencing tables.

Code engine smoke:

- Engine: `%APPDATA%\com.backendvisualmap.app\engines\codebase-memory-mcp.exe`
- Command: `cli index_repository` for `D:\meeting-overlay-assistant`.
- Result: pass.
- Project key: `D-meeting-overlay-assistant`.
- Indexed: 9,215 nodes and 40,215 edges.
- Query checks:
  - `search_graph` for `session` returned session domain and repository functions.
  - `get_architecture` returned routes, hotspots, layers, clusters, and file tree data.

Bug found and fixed:

- `database-memory` PostgreSQL adapter panicked when `information_schema.routines.routine_type` was `NULL` for extension aggregate routines such as pgvector `avg`.
- Fixed in `D:\project\db_mcp\crates\database-memory-core\src\adapters\postgres.rs` by handling nullable routine types.
- Verification: `D:\project\db_mcp` `cargo test` passed after the fix, and the release `database-memory.exe` was rebuilt and copied into the Backend Visual Map engine directory.

Product status:

- The real code graph and real PostgreSQL schema graph are both available for the same backend project.
- Remaining manual step: open Backend Visual Map, create/open a workspace for `D:\meeting-overlay-assistant`, select PostgreSQL, paste the local connection string, and run the UI indexing flow.
