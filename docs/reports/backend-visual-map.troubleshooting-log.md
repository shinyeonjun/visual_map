# Backend Visual Map Troubleshooting Log

Date: 2026-07-07
Scope: `D:\project\backend_map`

이 문서는 지금까지 실제 구현, 리팩토링, 실사용 QA, 설치/엔진 연동 중 발견한 문제를 "증상 -> 원인 -> 해결 -> 검증" 기준으로 모은 운영 로그다. phase별 완료 보고서는 `docs/reports/backend-visual-map.phase-*.md`에 있고, 이 파일은 다음 작업자가 같은 문제를 다시 밟지 않도록 만든 빠른 참조 문서다.

## Current Release Reality

- Windows 로컬 데스크톱 앱 기준으로 워크스페이스 생성, 코드 인덱싱, PostgreSQL 메타데이터 인덱싱, Workbench/Atlas 탐색, 재시작 복구까지 E2E 통과.
- `database-memory`는 metadata/catalog/DDL만 읽는다. row data 조회는 제품 원칙상 금지.
- DB 연결 문자열/비밀번호는 세션 입력으로만 사용하고 `workspace.json`, 앱 상태, 엔진 캐시에 저장하지 않는다.
- `codebase-memory-mcp.exe`, `database-memory.exe`는 내부 sidecar engine으로만 사용한다. Codex/Claude MCP 자동 등록은 하지 않는다.
- 공개 재배포는 `THIRD_PARTY_NOTICES.md`의 upstream engine license/copyright 고지가 끝나기 전까지 hold.

## Golden Verification Commands

기본 검증:

```powershell
cd D:\project\backend_map
npm run typecheck
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Tauri dev 실행:

```powershell
cd D:\project\backend_map
npm run tauri dev
```

WebView2 CDP를 열고 E2E를 돌릴 때:

```powershell
cd D:\project\backend_map
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333 --remote-allow-origins=*'
npm run tauri dev
```

다른 터미널에서 CDP 확인:

```powershell
Invoke-RestMethod http://127.0.0.1:9333/json
```

앱 데이터 초기화. 기존 데이터는 삭제하지 말고 백업으로 이동한다:

```powershell
$app = Join-Path $env:APPDATA 'com.backendvisualmap.app'
if (Test-Path $app) {
  Move-Item -LiteralPath $app -Destination "$app.backup-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
}
```

비밀정보 저장 여부 점검:

```powershell
rg "postgresql://|Password=|Pwd=|passwordStored: true|token\\s*=|secret\\s*=" "$env:APPDATA\com.backendvisualmap.app"
```

PostgreSQL 컨테이너 확인:

```powershell
docker ps
docker exec caps-postgresql-dev psql -U caps -d caps -tAc "select count(*) from information_schema.tables where table_schema='public' and table_type='BASE TABLE';"
```

## Troubleshooting Ledger

### 1. Vite Dev Port Conflict

Symptom:

- `npm run tauri dev` 실행 시 `Error: Port 1420 is already in use`.
- Tauri dev가 뜨기 전에 `beforeDevCommand`가 실패한다.

Cause:

- 이전 Vite/Tauri dev 프로세스가 살아 있거나 Claude/Codex가 백그라운드에서 Vite를 이미 실행 중이었다.

Fix:

- 기존 dev 프로세스를 종료한 뒤 다시 실행한다.
- 포트가 계속 잡히면 Windows 작업 관리자 또는 `Get-Process node`로 남은 Node/Vite 프로세스를 확인한다.

Verify:

```powershell
cd D:\project\backend_map
npm run tauri dev
```

### 2. WebView2 CDP Port Not Open

Symptom:

- E2E 자동화에서 `http://127.0.0.1:9222/json` 또는 CDP endpoint에 연결 실패.
- "WebView2 CDP 포트가 안 열림" 상태.

Cause:

- WebView2 remote debugging argument 없이 앱을 실행했다.
- 9222 포트가 이미 다른 프로세스에 잡혀 있거나 WebView2가 인자를 받지 못했다.

Fix:

- Tauri dev를 시작하기 전에 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`를 설정한다.
- 이번 프로젝트에서는 9333 포트가 안정적으로 동작했다.

Verify:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333 --remote-allow-origins=*'
npm run tauri dev
Invoke-RestMethod http://127.0.0.1:9333/json
```

### 3. AppData State Contamination

Symptom:

- 앱을 새로 켰는데 `phase71-final-smoke`, 이전 DB profile, 이전 스냅샷, 이전 canvas 상태가 남아 있다.
- 초기 상태 QA를 하려는데 "빈 화면"이 아니라 이전 워크스페이스가 자동 복구된다.

Cause:

- `%APPDATA%\com.backendvisualmap.app`에 이전 워크스페이스, engine cache, atlas snapshot이 남아 있었다.
- 재시작 복구 기능 자체는 정상이나 초기 QA에는 방해가 된다.

Fix:

- 앱 데이터를 삭제하지 말고 백업 폴더로 이동한 뒤 테스트한다.
- 사용했던 백업 예:
  - `%APPDATA%\com.backendvisualmap.app.backup-20260706-181225`
  - `%APPDATA%\com.backendvisualmap.app.backup-e2e-20260706-171020`

Verify:

- 앱 재실행 후 topbar가 "워크스페이스 없음" 또는 빈 초기 상태로 시작한다.
- `docs/reports/screenshots/qa-e2e/01-initial-empty.png` 참고.

### 4. Reset 후 Engine Missing

Symptom:

- 앱 초기화 후 상태바에 `codebase-memory 없음`, `database-memory 없음`.
- 코드 인덱싱 시 "필요한 엔진을 찾지 못했습니다" 류의 오류가 표시된다.

Cause:

- dev mode engine lookup이 `%APPDATA%\com.backendvisualmap.app\engines`에 의존했다.
- 앱 데이터 초기화로 appdata engine 폴더가 비면서, 실제 exe가 `src-tauri\target\debug\engines`에 있어도 찾지 못했다.

Fix:

- dev mode engine lookup 우선순위를 보정했다.
- 현재 의도:
  1. 실행 파일 주변 `engines`
  2. resource dir
  3. appdata fallback

Touched:

- `D:\project\backend_map\src-tauri\src\engine.rs`
- `D:\project\backend_map\src-tauri\src\engine_tests.rs`

Verify:

```powershell
cargo test --manifest-path D:\project\backend_map\src-tauri\Cargo.toml
```

### 5. `tauri-cdp-smoke.mjs` False Green

Symptom:

- CDP smoke script 내부에서 JavaScript exception이 났는데도 결과가 `{}`처럼 보이고 exit code 0으로 끝난다.
- 실제 화면은 실패했는데 자동화만 pass처럼 보인다.

Cause:

- `Runtime.evaluate` 결과의 `exceptionDetails`를 검사하지 않았다.

Fix:

- CDP evaluate 응답에 `exceptionDetails`가 있으면 즉시 throw하도록 수정.

Verify:

- 의도적으로 실패하는 selector/evaluate를 넣었을 때 smoke가 non-zero로 실패해야 한다.

### 6. Workbench/Atlas Toggle Text Wrapping

Symptom:

- 1440x900에서 topbar의 `작업대 / 아틀라스` 토글 텍스트가 두 줄로 꺾이고 잘린다.

Cause:

- flex item shrink와 text wrapping 제어가 부족했다.

Fix:

- toggle 텍스트에 `white-space: nowrap`, 고정 shrink 방지 스타일을 적용.

Touched:

- `D:\project\backend_map\src\styles\layout.css`

Verify:

- 1440x900 screenshot에서 `작업대`, `아틀라스`가 한 줄로 보인다.

### 7. Workbench에서 Atlas로 갔다가 돌아오지 못함

Symptom:

- Atlas 화면으로 전환 후 Workbench로 돌아가는 경로가 불명확하거나 동작하지 않았다.

Cause:

- view state와 topbar/rail 전환 책임이 분산되어 있었다.
- 일부 UI 요소가 버튼처럼 보이지만 실제 handler가 없었다.

Fix:

- Workbench/Atlas view state를 명확히 정리했다.
- topbar view switch와 로고/네비게이션 경로에서 Workbench 복귀를 확인했다.
- 클릭해도 동작 없는 장식 요소는 `aria-hidden` span으로 정리했다.

Verify:

- E2E에서 Workbench -> Atlas -> Workbench 왕복 2경로 확인.
- `docs/reports/backend-visual-map.qa-e2e.md` 참고.

### 8. Search Mode Dead Screen

Symptom:

- 검색어 없이 `검색` 모드를 누르면 노드 1개짜리 빈약한 화면 또는 "포커스가 필요합니다" 상태가 뜬다.

Cause:

- search mode가 focus target을 요구하지만, 검색어/선택 노드가 없는 상태에 대한 fallback이 약했다.

Fix:

- `search-focus` 생성 시 선택된 코드, 첫 코드 항목, 첫 테이블 순서로 fallback focus를 잡게 했다.
- 아직 UX적으로는 검색어 자동 포커스/빈 상태 안내가 더 필요하다.

Touched:

- `D:\project\backend_map\src\components\workbench\ModePanel.tsx`

Remaining:

- 검색 모드 진입 시 검색 입력 자동 포커스.
- 검색어 없는 상태에서는 "검색어를 입력하세요" 안내를 더 강하게 보여주기.

### 9. Canvas Collapsed Into One Vertical Line

Symptom:

- 큰 프로젝트를 로드하면 노드가 한 줄로 길게 늘어서고, 연결선이 읽히지 않는다.
- touchpad/scroll/zoom 동작도 체감상 불안정해 보인다.

Cause:

- raw graph를 그대로 보이려는 구조는 큰 프로젝트에서 실패한다.
- layer/row 계산이 실제 노드 밀도를 충분히 반영하지 못했다.
- fitView가 너무 자주 실행되어 사용자가 잡은 viewport를 덮어썼다.

Fix:

- raw full graph 직접 렌더링을 금지하고 grouped/focused map을 기본으로 유지.
- API, code, DB layer를 분리하고 layer offset 계산을 보정.
- `fitView`는 map identity가 바뀔 때만 실행.
- 소규모 맵은 fit zoom 상한을 둬서 과확대를 막음.
- React Flow pan/zoom, wheel/pinch, control button 동작을 재확인.

Touched:

- `D:\project\backend_map\src\components\WorkbenchCanvas.tsx`

Remaining:

- 최종 제품은 단순 graph viewer가 아니라 backend visual map이어야 한다.
- 큰 프로젝트 기본 화면은 "전체 노드"가 아니라 system domains, API groups, service groups, DB schemas 같은 요약 지도여야 한다.

### 10. Layer/Card Overlap

Symptom:

- API route 카드, code 카드, DB table 카드가 서로 겹친다.
- edge가 카드 위로 지나가서 구조가 더 복잡해 보인다.

Cause:

- layer 간 x offset이 고정값에 가까웠고, 각 layer의 실제 column count를 반영하지 못했다.

Fix:

- layer별 실제 column count 기반으로 누적 offset을 계산.
- API/code/table 레이어가 서로 침범하지 않게 배치.
- edge는 후보/포함/흐름 성격에 따라 style을 구분.

Verify:

- `docs/reports/screenshots/qa-e2e/06-mode-atlas.png`부터 `10-mode-search.png`까지 확인.

### 11. Huge Empty-State Button

Symptom:

- focus가 필요한 화면에서 `아틀라스 보기` 버튼이 세로로 길게 늘어나 giant rectangle처럼 보였다.

Cause:

- empty-state 내부 primary action이 flex/grid 부모 높이를 먹었다.

Fix:

- compact primary action에 fixed height, auto flex basis, max width를 적용.

Touched:

- `D:\project\backend_map\src\styles\buttons.css`

### 12. Atlas Mode Changed But Screen Did Not

Symptom:

- Atlas에서 모드를 바꿔도 중앙 지도와 패널이 거의 변하지 않아 "모드가 죽어 있음"처럼 보였다.

Cause:

- mode selection이 실제 projection/filter에 충분히 반영되지 않았다.

Fix:

- Atlas mode별로 표시 대상을 다르게 했다.
  - API: route/code 중심
  - dependency/schema: code + DB 중심
  - impact: DB/focus 중심
- band 번호와 label도 mode 기준으로 재계산.

Touched:

- `D:\project\backend_map\src\components\atlas\AtlasCanvas.tsx`

Remaining:

- Atlas group drilldown은 아직 미구현.

### 13. Atlas Fake Toolbar / Dead Buttons

Symptom:

- Atlas의 zoom/lock/floating tool처럼 보이는 요소들이 클릭 가능해 보이지만 실제 동작하지 않는다.

Cause:

- 시안성 장식과 실제 control이 button 스타일로 섞여 있었다.

Fix:

- 동작 없는 요소는 장식 span으로 변경.
- Workbench에서도 가짜 tab close/add, 중복 auto layout 버튼을 정리.

Principle:

- 누를 수 있어 보이면 실제로 동작해야 한다.
- 동작하지 않을 UI는 버튼처럼 보이면 안 된다.

### 14. Workspace Switch Stale Form/Status

Symptom:

- 워크스페이스를 바꿨는데 이전 workspace의 DB profile form 값, 성공/오류 메시지, visual map success message가 남는다.

Cause:

- workspace id 변경 시 local hook state reset이 충분하지 않았다.

Fix:

- workspace 전환 시 code inventory, DB profile, visual map status를 reset.
- DB profile이 없는 워크스페이스는 폼을 기본값으로 초기화.

Touched:

- `D:\project\backend_map\src\hooks\useDbProfiles.ts`
- `D:\project\backend_map\src\hooks\useCodeInventory.ts`
- `D:\project\backend_map\src\hooks\useVisualMap.ts`

Verify:

- E2E에서 workspace 전환 후 이전 상태 메시지가 남지 않는지 확인.

### 15. Missing Snapshot Shown As Hard Error

Symptom:

- 스냅샷이 아직 없을 뿐인데 `os error 2`가 "맵 생성 실패"처럼 표시된다.

Cause:

- 기존 로직은 `os error 3`만 snapshot missing으로 처리했다.
- atlas directory만 먼저 생긴 경우 file missing은 `os error 2`로 나온다.

Fix:

- `os error 2`도 snapshot missing 상태로 처리.

Touched:

- `D:\project\backend_map\src\hooks\useVisualMap.ts`

### 16. Legend Lied About Candidate Edges

Symptom:

- 범례가 점선을 "외래 키"처럼 설명했지만 실제 점선은 code->DB candidate relationship이었다.

Cause:

- UI label이 data trust rule보다 강한 확정 표현을 사용했다.

Fix:

- 범례를 "코드 흐름 / 포함 관계 / 후보 관계 (추정)"으로 정정.

Principle:

- 이름 기반 추정 링크는 절대 확정 관계처럼 표기하지 않는다.

### 17. SQLite DDL Smoke vs PostgreSQL Schema Reality

Symptom:

- `meeting-overlay-assistant`의 PostgreSQL DDL을 SQLite DDL source로 읽으면 일부 객체가 빠지거나 parser가 약해 보인다.

Cause:

- 실제 DDL은 PostgreSQL 문법이다.
- `CREATE EXTENSION`, `DO $$`, `ALTER ... IF NOT EXISTS`, PostgreSQL-specific type/default/index 문법은 SQLite DDL adapter의 목표가 아니다.

Fix:

- 해당 프로젝트의 정확한 DB 검증은 live PostgreSQL metadata source로 한다.
- SQLite DDL은 SQLite 호환 DDL 또는 최소 DDL smoke 용도로만 사용한다.

Verify:

- PostgreSQL live smoke에서 public base tables 20개, columns 221개 확인.
- DSN은 문서/로그에서 항상 redacted:

```text
postgresql://caps:[REDACTED]@127.0.0.1:55432/caps
```

### 18. PostgreSQL Smoke "왜 안 됨?" Confusion

Symptom:

- RDB 테스트 때 PostgreSQL은 됐는데 final audit에는 PostgreSQL smoke가 skip/hold처럼 보였다.

Cause:

- 한 시점에는 Claude 세션에서 PostgreSQL env가 없어서 skip으로 기록했다.
- 이후 Docker Desktop에서 `caps-postgresql-dev`가 떠 있고 env를 주입한 상태로 재실행해 pass가 됐다.

Current:

- PostgreSQL blocker는 해소됨.
- 남은 release blocker는 third-party license notice.

Verify:

```powershell
$env:BACKEND_MAP_TEST_POSTGRES_URL='postgresql://caps:[REDACTED]@127.0.0.1:55432/caps'
scripts\smoke-rdb-productization.ps1 -DatabaseMemory src-tauri\engines\database-memory.exe
```

### 19. Secret Persistence False Alarm

Symptom:

- 앱 데이터에서 DB명/스키마명이 반복된 문자열이 보여 비밀번호가 저장된 것처럼 의심됨.

Cause:

- database name과 schema name이 같은 값이라 graph/cache key에 `<database>:<schema>` 형태로 나타난 것.
- connection string/password/host/port 저장이 아니었다.

Fix:

- secret scan을 connection string, password assignment, token/secret pattern 기준으로 재확인.
- 문서에는 DSN/password를 redacted로만 기록.

Verify:

```powershell
rg "postgresql://|Password=|Pwd=|passwordStored: true|token\\s*=|secret\\s*=" "$env:APPDATA\com.backendvisualmap.app"
```

### 20. Codebase MCP Stale Index

Symptom:

- codebase-memory MCP로 재인덱싱을 시도했지만 최신 파일 변경이 반영되지 않았다.
- `.git`이 없거나 project identity가 꼬이면 재의존성 pipeline이 완료되지 않는 것으로 보였다.

Cause:

- 현재 앱 폴더가 독립 git repo가 아닌 상태였고, MCP project cache와 실제 작업 폴더가 깔끔히 격리되지 않았다.
- 여러 프로젝트를 한 세션에서 다루며 graph freshness 보장이 약했다.

Decision:

- 이 앱 개발에서는 codebase-memory MCP를 작업 도구로 쓰지 않는다.
- 제품 내부에서는 upstream `codebase-memory-mcp.exe`를 "사용자 repo indexing sidecar"로만 사용한다.
- Codex/Claude에 자동 MCP 등록하지 않는다.

Practical Rule:

- 앱 구현/검증은 `rg`, 파일 읽기, 테스트, E2E로 확인한다.
- 제품 sidecar smoke는 별도 engine command로만 확인한다.

### 21. Claude Limit Mid-Patch

Symptom:

- `D:\project\backend_map\design\ui-concepts\Backend Visual Map (standalone).html` 기준으로 UI를 맞추는 도중 Claude가 limit에 걸려 변경이 중간에 멈췄다.

Cause:

- 대형 UI/CSS 변경과 E2E를 한 번에 맡기면서 토큰 사용량이 커졌다.

Fix:

- 남은 patch를 작게 회수:
  - `ViewOptionsPanel` 연결
  - minimal CSS 추가
  - typecheck/build/cargo test
- 이후부터는 UI 개편도 "한 화면/한 상태/한 검증" 단위로 쪼개야 한다.

Verify:

```powershell
npm run typecheck
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

### 22. Dark Standalone HTML Concept Drift

Symptom:

- 실제 앱 UI가 `design/ui-concepts/Backend Visual Map (standalone).html`의 완성형 방향과 점점 달라졌다.
- 특히 중앙 지도는 "백엔드 지도"보다 일반 graph viewer처럼 보였다.

Cause:

- 구현 phase가 엔진 연결/검증 중심으로 빠르게 진행되면서 final visual language 정착이 뒤로 밀렸다.

Decision:

- standalone HTML을 최종 UI 기준점으로 삼는다.
- 단, HTML 파일을 통째로 복붙하지 않는다. 제품 코드에 맞게 다음 방향을 점진적으로 이식한다:
  - dark dense workspace
  - 좌측 repo/db source rail
  - 중앙 API -> code -> DB layered/swimlane map
  - 우측 mode/inspector rail
  - 후보 링크 confidence/evidence 표시
  - 대규모 프로젝트는 raw graph 대신 grouped/focused projection

### 23. Large Project Visualization Product Rule

Symptom:

- 큰 프로젝트에서 노드가 많아지면 시각화가 예쁘지 않고 이해하기 어렵다.

Root Product Insight:

- 사용자는 "그래프 전체"를 보고 싶은 게 아니라 "백엔드 구조를 빠르게 이해하는 지도"를 원한다.

Rule:

- 기본 화면은 raw nodes가 아니라 다음 projection 중 하나여야 한다:
  - System Atlas: route group, service group, DB schema/table group
  - API Flow: selected API request path
  - Table Usage: selected table을 쓰는 code candidates
  - Column Impact: selected column blast radius
  - Search Focus: query result around selected target

Non-goal:

- Neo4j-style giant hairball graph viewer.

### 24. Release Hold: License Notices

Symptom:

- 앱 기능은 local demo 기준으로 통과했지만 release decision이 hold.

Cause:

- bundled upstream engines의 license/copyright 원문 고지가 완성되지 않았다.

Fix Required Before Public Distribution:

- `THIRD_PARTY_NOTICES.md`에 다음을 정확히 추가:
  - `codebase-memory-mcp` upstream license text/copyright
  - `database-memory` own license/copyright
  - bundled binary redistribution terms

Current Decision:

- 개인/로컬/전시 demo는 가능.
- 공개 설치본 배포는 license notice 완료 전까지 hold.

## E2E Baseline

Full QA report:

- `D:\project\backend_map\docs\reports\backend-visual-map.qa-e2e.md`

Screenshot folder:

- `D:\project\backend_map\docs\reports\screenshots\qa-e2e`

Passed flow:

1. AppData reset from empty state.
2. Workspace create for `D:\meeting-overlay-assistant`.
3. Code indexing and load.
4. PostgreSQL metadata connection test.
5. DB metadata indexing and load.
6. Workbench modes: Atlas, API Flow, Table Usage, Column Impact, Search.
7. Workbench <-> Atlas roundtrip.
8. Wheel/button zoom and pan.
9. Search focus.
10. Restart restore.
11. Secret persistence scan.

## Known Remaining Work

High priority:

- Atlas group drilldown.
- Database engine column type output so UI stops showing `타입 ?`.
- Search mode empty-state guidance and autofocus.
- Status bar should prefer most recent operation, not fixed priority.
- Final dark UI alignment with standalone concept.
- Third-party license notices before public redistribution.

Medium priority:

- Better code->DB candidate confidence and evidence.
- Corrupt `workspace.json` recovery UX.
- Remove or wire remaining unused commands.
- GitHub URL clone UX polish.

Do not do:

- Do not render raw full graph for large repos.
- Do not persist DB passwords or full connection strings.
- Do not query DB row data.
- Do not auto-register MCP servers into Codex, Claude, or user AI tools.

