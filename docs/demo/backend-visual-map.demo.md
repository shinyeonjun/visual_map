# Backend Visual Map 3-Minute Demo

## Goal

Show how Backend Visual Map turns a real backend repository plus DB metadata into a navigable Korean-first architecture map without reading DB row data.

## Demo Inputs

- Repository: `D:\meeting-overlay-assistant`
- DB source: SQLite DDL
- DDL path: `D:\meeting-overlay-assistant\server\app\infrastructure\persistence\postgresql\drawsql\030_drawsql_schema.sql`
- Workspace name: `meeting-overlay-demo`

## Three-Minute Script

1. Open Backend Visual Map.
   - Point out that `codebase-memory` and `database-memory` show as 정상.
   - Say: "이 앱은 로컬 코드와 DB 메타데이터만 인덱싱합니다. row data는 읽지 않습니다."

2. In Workbench, create or open the demo workspace.
   - Name: `meeting-overlay-demo`
   - Repo path: `D:\meeting-overlay-assistant`
   - Click `생성` if the workspace does not exist.

3. Index and load code metadata.
   - Click `저장소 인덱싱`.
   - Click `코드 불러오기`.
   - Expected demo counts from the Phase 72 smoke: 50 API routes, 80 services, 12 files.

4. Save, index, and load DB metadata.
   - Source: `SQLite DDL`
   - Profile name: `meeting-overlay-ddl`
   - DDL path: `D:\meeting-overlay-assistant\server\app\infrastructure\persistence\postgresql\drawsql\030_drawsql_schema.sql`
   - Click `DB 프로필 저장`, `메타데이터 인덱싱`, then `테이블 불러오기`.
   - Expected demo count from the Phase 72 smoke: 18 tables.

5. Show Workbench map.
   - The canvas should show grouped nodes, not a raw full graph.
   - Point out that code-to-DB links are candidate links unless direct evidence proves them.

6. Switch to Atlas.
   - Show the three bands: API routes, code, and database schema.
   - Use the overview to explain the product in one sentence: "요청 흐름과 DB 스키마를 한 화면에서 좁혀 보는 로컬 백엔드 지도입니다."

7. Show the three focused stories.
   - API Flow: choose the API mode and use `/api/v1/sessions` as the spoken example.
   - Table Usage: choose dependency/schema mode and point to `sessions`.
   - Column Impact: choose impact mode and point to `sessions.id`.

## Screenshot References

- Workbench loaded: `docs/reports/screenshots/phase-72/workbench-loaded.png`
- Atlas overview: `docs/reports/screenshots/phase-72/atlas-grouped.png`
- API Flow: `docs/reports/screenshots/phase-72/atlas-api-flow.png`
- Table Usage: `docs/reports/screenshots/phase-72/atlas-table-usage.png`
- Column Impact: `docs/reports/screenshots/phase-72/atlas-column-impact.png`

## Timing

- 0:00-0:30: app purpose and local/privacy boundary.
- 0:30-1:20: workspace, code metadata, DB metadata.
- 1:20-2:10: Workbench grouped map.
- 2:10-3:00: Atlas overview plus API/table/column focused story.

## Pass Criteria

- The demo can be repeated in 3 minutes with the paths above.
- Counts are shown honestly from metadata/index output.
- The canvas never shows a fake current state while data is missing or stale.
- No DB row data is queried.
- DB passwords are not entered or persisted.
- No MCP server is registered into Codex, Claude, or another AI tool.
