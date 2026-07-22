# Backend Visual Map Final Product Target

Status: Product target baseline
Scale: Large
Date: 2026-07-06

## One-Line Goal

Backend Visual Map은 Git 저장소와 RDB 메타데이터를 읽어, 백엔드 개발자가 API, 코드, DB 관계와 변경 영향 범위를 시각적으로 이해하게 해주는 로컬 데스크탑 지도다.

## Finished Product Definition

완성본은 그래프 뷰어가 아니다. 완성본은 원본 코드 그래프와 DB 스키마 그래프를 개발자가 바로 판단할 수 있는 백엔드 지도로 변환하는 제품이다.

사용자는 노드와 엣지를 직접 해석하지 않는다. 사용자는 질문을 고른다.

- 이 API는 어디까지 이어지나?
- 이 테이블은 코드 어디서 쓰이나?
- 이 컬럼을 바꾸면 뭐가 깨질 수 있나?
- 이 프로젝트의 큰 백엔드 구조는 어떻게 생겼나?

## Primary Users

### Junior Backend Developer

Needs:

- 처음 보는 프로젝트에서 어디부터 읽어야 하는지 알고 싶다.
- API 요청이 어떤 함수와 테이블로 이어지는지 보고 싶다.
- 무작정 파일 검색을 반복하고 싶지 않다.

Product promise:

- API 중심 흐름 지도로 읽을 파일 순서를 알려준다.

### Mid-Level Backend Developer

Needs:

- 기능 수정 전 영향 범위를 빠르게 보고 싶다.
- 특정 테이블/컬럼의 코드 사용처를 찾고 싶다.
- 레거시 프로젝트에서 숨은 결합을 줄이고 싶다.

Product promise:

- 테이블 사용처와 컬럼 영향도를 근거와 함께 보여준다.

### Senior Backend Developer

Needs:

- 큰 구조, 결합도, 위험 영역을 빠르게 파악하고 싶다.
- 팀 온보딩/리뷰/리팩토링 전에 공통 지도를 만들고 싶다.
- 추론 결과가 왜 나왔는지 검증하고 싶다.

Product promise:

- 전체 아키텍처 요약, 근거 있는 후보 링크, 위험 경고를 제공한다.

## Core UX Principle

처음은 그룹 지도, 클릭하면 상세 지도, 검색하면 포커스 지도, 변경하면 영향도 지도.

```text
전체 백엔드
  -> 도메인/그룹
    -> API 흐름
      -> 함수/파일/테이블
        -> 컬럼/제약/인덱스
```

원본 그래프를 그대로 렌더링하지 않는다. 큰 프로젝트에서도 기본 화면은 20~40개 정도의 읽을 수 있는 그룹 노드로 시작한다.

## Main Workflows

### 1. First Run

Goal:

- 사용자가 3~5분 안에 첫 지도를 볼 수 있어야 한다.

Flow:

```text
저장소 선택
  -> 워크스페이스 생성
  -> 코드 인덱싱
  -> DB 연결 또는 DDL/SQLite 선택
  -> DB 메타데이터 인덱싱
  -> 백엔드 지도 생성
```

Requirements:

- 폴더 선택 버튼이 있어야 한다.
- GitHub URL 입력은 지원하되, 내부적으로 로컬 clone 후 인덱싱한다.
- 각 단계는 로딩, 성공, 실패 상태를 명확히 보여준다.
- 실패하면 다음 행동을 알려준다.

### 2. Architecture Atlas

Goal:

- 프로젝트의 큰 구조를 한눈에 본다.

Default map:

```text
API 그룹 -> 핸들러/서비스 그룹 -> 저장소/쿼리 그룹 -> DB 그룹
```

Requirements:

- 도메인/폴더/라우트/DB 스키마 기준으로 그룹핑한다.
- 너무 많은 노드는 접고 count로 보여준다.
- 그룹 클릭 시 해당 그룹 내부 지도로 들어간다.

### 3. API Flow

Goal:

- 하나의 API 요청이 어떤 코드와 DB 후보로 이어지는지 본다.

Map:

```text
Route -> Handler -> Service/Function -> Repository/Query Candidate -> Table/Column Candidate
```

Requirements:

- API route 목록에서 선택할 수 있어야 한다.
- 코드 호출 관계는 codebase-memory 결과를 우선 사용한다.
- DB 연결은 확정이 아닌 candidate link로 표시한다.
- candidate edge 클릭 시 evidence를 보여준다.

### 4. Table Usage

Goal:

- 특정 테이블이 어떤 코드/API에서 사용될 가능성이 있는지 본다.

Map:

```text
Table -> Columns/FKs/Indexes
Table -> Candidate Files/Functions/APIs
```

Requirements:

- DB 내부 관계는 확정 관계로 표시한다.
- 코드 사용처는 confidence와 evidence를 함께 표시한다.

### 5. Column Impact

Goal:

- 컬럼 변경/삭제/타입 변경 전 영향 범위를 본다.

Map:

```text
Column -> Constraints/FKs/Indexes -> Related Tables
Column -> Candidate Code References
```

Requirements:

- FK, PK, unique, index 영향은 DB 메타데이터 기준으로 계산한다.
- 코드 후보 영향은 confidence/evidence 기반으로 표시한다.
- 위험 요약을 우측 인스펙터에 보여준다.

### 6. Search Focus

Goal:

- "session" 같은 키워드로 API, 코드, DB를 한 번에 찾고 즉시 지도로 본다.

Requirements:

- 검색 결과는 API, 코드, 파일, 테이블, 컬럼으로 그룹핑한다.
- 결과 선택 시 local focus map을 생성한다.

## Visual Rules

- Raw graph direct render 금지.
- 기본 지도는 그룹 노드 중심.
- 상세 지도는 선택한 focus의 1~2 hop 중심.
- 확정 관계와 추론 관계를 시각적으로 분리한다.
- candidate link는 점선으로 표시한다.
- confidence는 high/medium/low로 단순화한다.
- edge 클릭 시 evidence를 보여준다.
- 화면이 비어 있으면 다음 행동을 안내한다.

## Data Trust Rules

### Confirmed Data

다음은 확정 데이터로 취급한다.

- DB table
- DB column
- primary key
- foreign key
- unique constraint
- index
- code function/class/file/module
- code route relation reported by codebase-memory
- code call relation only when codebase-memory includes a confidence score of 0.85 or higher

### Candidate Data

다음은 추론 데이터로 취급한다.

- code -> table usage
- code -> column usage
- route -> table usage
- service -> table usage
- repository -> table usage unless SQL/table evidence is exact
- code call relations scored from 0.70 through 0.84 by codebase-memory

Candidate data must include:

- confidence
- evidence list
- source node
- target DB object

Never label candidate links as confirmed in v1.

CALLS below 0.70 or without an engine confidence score are unknown evidence. They do not enter a confirmed reading path or add disconnected neighbor cards. Existing snapshots without scored CALLS require a code re-read.

## Required Product Capabilities

### Workspace

- create workspace from local folder
- create workspace from GitHub URL by cloning locally
- reopen last workspace
- list recent workspaces
- persist code and DB inventory snapshots
- recover UI state after restart

### Engine Integration

- detect bundled `codebase-memory-mcp.exe`
- detect bundled `database-memory.exe`
- show version/availability
- run indexing in background
- redact command output before storage
- timeout long-running engine calls

### RDB Sources

Supported:

- SQLite file
- SQLite DDL file/directory
- PostgreSQL
- MySQL/MariaDB
- SQL Server
- Oracle

Hard rule:

- metadata/catalog/PRAGMA only
- no row data reads
- no arbitrary SQL console
- no password persistence in v1

### Maps

Required map modes:

- Architecture Atlas
- API Flow
- Table Usage
- Column Impact
- Search Focus

Deferred map modes:

- PR Impact
- Migration Risk
- Test Scope
- Architecture Drift
- Team Shared Map

## Non-Goals

- 3D graph
- BI dashboard
- SQL query console
- DB row data browsing
- automatic code modification
- automatic migration execution
- team cloud sync in v1
- multi-user collaboration in v1
- merging codebase-memory and database-memory into one engine
- showing every raw node at once

## UX Completion Criteria

The app is not complete until all of these are true:

- A new user can create a workspace without reading docs.
- A user can select a repo with a folder picker.
- A user can paste a GitHub URL and let the app clone/index it.
- A user can connect/index at least one DB profile.
- Every long operation has visible progress.
- Every failure state explains the next action.
- Restarting the app restores the last workspace and available snapshots.
- Workbench and Atlas navigation is reliable.
- Large projects do not render as a one-line graph.
- Empty state screens never look broken.

## Large Project Criteria

Target large project baseline:

- 10k+ code graph nodes
- 40k+ code graph edges
- 20+ DB tables
- 200+ DB columns

Required behavior:

- initial map renders as grouped atlas, not raw nodes
- default visible node count stays under 40
- focus maps stay under a configured cap
- broad results ask the user to narrow focus
- UI remains responsive during indexing and map generation

## Release Readiness Criteria

Release candidate requires:

- `npm run typecheck` passes
- `npm run build` passes
- `cargo test` passes
- real code engine smoke passes
- real DB engine smoke passes for SQLite DDL and PostgreSQL
- at least one network DB smoke beyond PostgreSQL passes before public release
- Windows installer includes sidecars
- fresh install can run without PATH setup
- secret persistence audit passes
- 1440x900 screenshot QA passes for Workbench and Atlas
- meeting-overlay-assistant product smoke passes with both code and DB loaded

## Implementation Priority

Do next in this order:

1. Refactor oversized files without changing behavior.
2. Fix workspace/session restore and navigation reliability.
3. Replace raw graph rendering with grouped visual projection.
4. Stabilize API Flow, Table Usage, and Column Impact maps.
5. Add GitHub URL clone flow.
6. Improve candidate link evidence and confidence display.
7. Run real project QA with `meeting-overlay-assistant`.
8. Package Windows release with sidecars.

## Codex/Claude Baseline Prompt

Use this before future implementation work:

```text
Read D:\project\backend_map\docs\plans\backend-visual-map-final-product.md first.
Treat it as the product target baseline.

Implement only the requested phase or fix.
Do not render raw full graphs directly.
Do not add row-data DB access.
Do not persist DB passwords.
Keep candidate code-to-DB links visibly marked with confidence and evidence.
Run the smallest relevant checks and write a report when the phase plan asks for one.
```
