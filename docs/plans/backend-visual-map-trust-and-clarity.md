# Backend Visual Map: 신뢰성 + 직관 시각화 디벨롭 계획 (Trust & Clarity)

Status: Implemented (T1-T5)
Scale: Medium (Phase 5개, 각각 small patch)
Date: 2026-07-07
Source of truth: `docs/plans/backend-visual-map-final-product.md`

## Goal

백엔드 개발자가 맵을 보고 "이건 확정 사실, 이건 추정"을 즉시 구분하고, API 하나를 찍으면
실제 호출/제약 데이터 기반의 흐름을 이해할 수 있게 만든다.

지금 제품의 근본 약점은 그림이 아니라 **근거 품질**이다:

| 표시 중인 것 | 실제 근거 | 문제 |
| --- | --- | --- |
| API 흐름 route→code 엣지 (`code_flow`) | 이름 토큰 공유 휴리스틱 | 실제 호출 관계가 아님. "같은 도메인 토큰 공유"가 근거 |
| 코드→테이블 후보 (`candidate_uses`) | 이름 부분일치 | 후보로 정직하게 표시 중 (유지) |
| 컬럼 영향도 | 이름 매칭 + PK/FK 불리언 | `created_at` 같은 흔한 컬럼에서 후보 0개. FK가 **어느 테이블을 참조하는지** 없음 |
| 컬럼 타입 | `find-column` 라인 출력 | 타입 미포함 → UI에 `타입 ?` |

## 근거 1: 커뮤니티 조사 (2026-07 리서치 요약)

- 처음 보는 코드베이스에서 원하는 것은 전체 구조가 아니라 **endpoint를 따라가는 data flow** (r/ExperiencedDevs 반복 조언).
- 자동 다이어그램이 안 쓰이는 이유: 모든 branch를 그리면 도움이 안 됨 → **포커스된 지도**가 핵심 (HN).
- 다이어그램 신뢰 문제: 업데이트 안 되면 버려짐 → **스냅샷 시각/근거/확정·후보 구분** 상시 표시 필요.
- 실무에서 돈 되는 질문: **"이거 바꾸면 뭐 터져?"** (impact analysis).
- 결론: 예쁜 전체 그래프가 아니라 **변경 전에 무서운 부분을 빨리 찾는 지도**. 단, 근거 없는 단정은 신뢰를 파괴한다.

## 근거 2: 엔진 능력 실측 (2026-07-07 검증 완료)

두 엔진 모두 지금 앱이 안 쓰는 **확정 데이터**를 이미 제공한다. 아래는 실제 실행으로 검증한 출력이다.

### database-memory `describe-table` (신규 활용)

```text
database-memory describe-table <alias> <table-name> --format json --cache-path <graph.sqlite>
# alias 형식은 기존 db_snapshot_alias()와 동일: "{source}:{profile_id}" 예) "postgres:caps-postgres-...-1"
```

실측 출력 (caps PostgreSQL, `accounts` 테이블):

```json
{
  "table": "accounts",
  "columns": [ { "name": "id", "nullable": false, "type": "text" }, ... ],
  "primary_key": ["id"],
  "foreign_keys": {
    "outbound": [
      { "name": "accounts_workspace_id_fkey", "columns": ["workspace_id"],
        "referenced_table": "workspaces", "referenced_columns": ["id"], "table": "accounts" }
    ],
    "inbound": [
      { "name": "sessions_account_id_fkey", "columns": ["account_id"],
        "referenced_table": "accounts", "referenced_columns": ["id"], "table": "sessions" }, ...
    ]
  },
  "indexes": [ { "name": "accounts_pkey", "columns": ["id"], "primary": true, "unique": true } ],
  "capability_warnings": ["cross-object dependency metadata is partially tracked by the postgres adapter."]
}
```

→ 컬럼 타입(`타입 ?` 해결), PK, **명명된 FK 제약 + 참조 대상 테이블/컬럼**, 인덱스. 전부 catalog 확정 데이터.

### codebase-memory `query_graph` CALLS (신규 활용)

```text
codebase-memory-mcp cli query_graph '{"project":"<codeProject>","query":"MATCH (a)-[r:CALLS]->(b) RETURN a.qualified_name, b.qualified_name LIMIT 5000","cache_dir":"<code/cache>"}'
# env CBM_CACHE_DIR=<code/cache> 도 함께 설정 (기존 run_code_query 패턴 그대로)
```

실측 출력:

```json
{"columns":["a.qualified_name","b.qualified_name"],
 "rows":[["...DetailPanel.DetailPanel","...DetailPanel.loadDetail"], ...],"total":5}
```

→ 실제 호출 그래프. 인벤토리 항목 id가 qualified_name이므로 그대로 조인 가능.

### 함정 (실측에서 걸린 것)

- JSON 인자 안의 Windows 백슬래시 경로는 이스케이프 깨짐 → 기존 Rust처럼 serde_json으로 직렬화하면 안전. 수동 문자열 조립 금지.
- `project`는 workspace 이름이 아니라 `workspace.code_project` (예: `D-meeting-overlay-assistant`).
- `database-memory`는 `--help`/`help` 미지원. 인자 없이 실행하면 usage 출력.

## Success Criteria

1. 테이블 로드 후 UI에 `타입 ?`가 사라지고 실제 타입이 보인다 (describe 지원 소스 기준).
2. 컬럼 영향도에서 FK 컬럼 선택 시 **참조 대상/참조하는 테이블이 확정 엣지(초록 실선)** 로 보이고, 근거에 제약 이름(`sessions_account_id_fkey`)이 표시된다. 기존 "FK 대상 테이블 정보는 현재 스냅샷에 없어..." 경고가 해당 케이스에서 사라진다.
3. API 흐름에서 route→code 엣지가 **실제 CALLS 관계**일 때 확정(파랑 실선)으로 표시되고, 인스펙터 근거가 "호출 그래프"가 된다. 토큰 매칭은 CALLS가 없을 때만 fallback으로 남고 "추론"으로 구분 표시된다.
4. 확정/후보 시각 문법이 일관됨: 확정=실선(파랑 호출/초록 FK·제약), 후보=주황 점선+신뢰도 라벨. 범례와 인스펙터 라벨이 실제 렌더와 1:1.
5. 기존 E2E 플로우(초기화→생성→인덱싱→로드→5모드→재시작 복구)가 그대로 통과. row-data 접근/비밀번호 저장/raw full graph 없음 유지.

## In Scope / Out of Scope

In:
- DB 인벤토리 강화(describe-table), 스냅샷 링크 모델, CALLS 엣지, 시각 문법·인스펙터 근거 정리.

Out (이번 계획에서 금지):
- SQL 파싱으로 코드→테이블 확정 링크 만들기 (엔진 근거 없이 단정 금지 — 후보 유지)
- PR Impact / Migration Risk (deferred-after-v1)
- 이름 변경(아틀라스→지도) — 별도 결정
- DB row data, SQL 콘솔, 비밀번호 저장, MCP 등록 (하드 룰)

## Affected Areas

- Rust: `src-tauri/src/workspace/model.rs`, `workspace/db.rs`, `workspace/code.rs`, `atlas/model.rs`, `atlas/snapshot.rs`, `atlas/visual_map.rs`, `atlas/linker.rs`(변경 없음, 후보 유지), 각 tests
- TS: `src/types/workspace.ts`, `src/types/visual-map.ts`, `src/inventory/inventorySnapshot.ts`, `src/inventory/snapshotRestore.ts`, `src/components/workbench/InspectorPanel.tsx`, `src/components/atlas/AtlasCanvas.tsx`, `src/styles/canvas.css`

## Phase 계획 (각 Phase = 독립 small patch, 순서 고정)

### Phase T1: DB 인벤토리에 describe-table 통합

Tasks:
- `db_inventory()`에서 find-table로 테이블 목록 확보 후, 테이블별 `describe-table` 호출 (상한 40개, 개당 timeout 30s, 실패 테이블은 기존 라인 파싱 결과로 fallback).
- `DbInventoryColumn`에 `nullable: Option<bool>` 추가, `data_type` 채움, `is_primary_key`는 `primary_key` 배열로, `is_foreign_key`는 outbound FK 컬럼 포함 여부로 판정.
- `DbInventoryTable`에 `foreign_keys: Vec<DbForeignKey>` 추가:
  `{ name, columns: Vec<String>, referenced_schema: Option<String>, referenced_table, referenced_columns: Vec<String> }` (outbound만 저장 — inbound는 상대 테이블의 outbound에서 유도).
- TS `DbInventoryTable`/`DbInventoryColumn` 타입 동기화.
- describe 미지원/실패 소스에서 기존 동작 유지 (하위호환).

Done when: `테이블 불러오기` 후 컬럼 타입이 보이고, DbInventory JSON에 FK가 들어온다.
Checks: `cargo test`(describe 파싱 단위 테스트 추가), `npm run typecheck`, 실 PostgreSQL 또는 DDL 스모크.

### Phase T2: 스냅샷 링크 모델 + FK 확정 엣지

Tasks:
- `InventorySnapshot`에 `#[serde(default)] links: Vec<SnapshotLink>` 추가:
  `{ id, from, to, kind, label: Option<String> }` — kind는 `"db_fk" | "code_call"`. 기존 스냅샷 파일과 하위호환(serde default, TS optional).
- 프론트 `buildInventorySnapshot`: FK → `db_fk` 링크 (from=`db:table:<key>` 또는 컬럼 단위 `db:column:<key>:<col>`, to=참조 테이블/컬럼 id, label=제약 이름). 참조 대상이 인벤토리에 없으면 링크 생략.
- `visual_map.rs`: links의 `db_fk`를 확정 엣지 kind `db_fk`로 렌더. evidence = `{kind:"db-constraint", text:"<제약이름>: <t1>.<c1> → <t2>.<c2>"}`.
  - `table_detail_map`: 해당 테이블의 in/outbound FK 상대 테이블 노드+엣지 추가 (캡 준수).
  - `column_impact_map`: 컬럼이 FK 구성원이면 참조 대상/참조하는 테이블·컬럼을 확정으로 추가하고, 기존 "FK 대상 테이블 정보 없음" 경고는 links 부재 시에만 출력.
- `snapshotRestore.ts`: FK를 DbInventory로 복원.

Done when: FK 있는 테이블/컬럼 포커스 맵에 초록 실선 + 제약 이름 근거가 보인다.
Checks: `cargo test`(visual_map fk 테스트), `npm run typecheck`, `npm run build`, 재시작 복구 스모크(구 스냅샷 로드 OK).

### Phase T3: 코드 CALLS 확정 엣지 + API 흐름 실데이터화

Tasks:
- `code_inventory()`에서 `query_graph`로 CALLS 조회 (LIMIT 5000 캡, 실패 시 빈 목록 — 기능 저하 없이 fallback).
- `CodeInventory`에 `calls: Vec<CodeCall> { from, to }` 추가 (인벤토리에 포함된 항목 간 관계만 필터해서 축소).
- 프론트 스냅샷 직렬화: calls → `code_call` 링크 (`code:<id>` ↔ `code:<id>`).
- `api_flow_map`: focus route에서 `code_call` 링크로 1~2 hop BFS (캡 준수) → 확정 엣지 kind `code_call`. CALLS 결과가 0일 때만 기존 토큰 매칭 fallback (kind `code_flow` 유지, evidence 문구 "이름 기반 추론"으로 명시).
- atlas overview 그룹 엣지에도 code_call 집계 반영(그룹 간 확정 호출 수 — 선택 사항, 캡 내에서).

Done when: `/api/v1/...` route 선택 시 실제 호출 체인이 확정 엣지로 보인다.
Checks: `cargo test`, `npm run typecheck`, meeting-overlay 실앱 스모크 (API 흐름 노드/엣지 kind 확인).

### Phase T4: 시각 문법 + 인스펙터 근거 정리

Tasks:
- 엣지 스타일 확정: `code_call` 파랑 실선, `db_fk`/`db_constraint` 초록 실선, `contains` 회색 얇은 실선, `candidate_*` 주황 점선+애니메이션, (fallback) `code_flow` 파랑 점선(추론임을 시각화). 범례를 실제 kind와 1:1로 갱신.
- InspectorPanel:
  - `relationshipSourceLabel`: `db_fk`/`code_call` → "확정", `code_flow` → "추론", `candidate_*` → "후보".
  - 엣지 선택 시 from/to를 raw id가 아니라 노드 title로 표시.
  - 근거 섹션에 제약 이름/호출 관계 원문 표시. 후보는 기존 신뢰도 배지 유지.
- 컬럼 노드 subtitle에 타입/nullable 반영 (T1 데이터).
- 스윔레인 감각: 캔버스 각 레이어 열 상단에 레인 라벨(API/코드/DB) 노드(비인터랙티브) 추가 — 시안(레이어드 맵) 문법. 캡/성능 영향 없음.

Done when: 처음 보는 사람이 범례만 보고 실선=확정, 점선=추정을 구분하고, 엣지 클릭 시 "왜"가 보인다.
Checks: `npm run typecheck`, `npm run build`, 1440x900 스크린샷 QA.

### Phase T5: E2E 재검증 + 보고서

- 기존 QA 플로우 재실행 (앱 데이터 백업·초기화 → 생성 → 코드/DB 인덱싱·로드 → 5모드 → 왕복 → 재시작 복구 → 보안 rg 스캔).
- 신규 확인 항목: FK 엣지 근거, CALLS 엣지, 타입 표시, 구 스냅샷(links 없음) 하위호환.
- `docs/reports/backend-visual-map.trust-and-clarity.md` 작성 (PASS/FAIL/SKIP 명시).

## 성능/안전 가드

- describe-table 테이블 상한 40, CALLS LIMIT 5000, 맵 노드/엣지 캡 기존 유지.
- 모든 신규 엔진 호출은 기존 `run_engine_command*` 경로 사용 (redaction/timeout/인자 검증 상속).
- 스냅샷 신규 필드는 전부 `serde(default)`/TS optional — 구 스냅샷 로드 깨지지 않아야 함 (T2 체크에 포함).
- links에도 저장 전 redaction 적용됨 (`save_inventory_snapshot` 기존 경로).

## Test Commands

```powershell
npm run typecheck
npm run build
cd src-tauri; cargo test
# 실앱 스모크 (CDP)
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"
$env:BACKEND_VISUAL_MAP_ENGINE_DIR="D:\project\backend_map\src-tauri\engines"
npm run tauri dev
node scripts/tauri-cdp-smoke.mjs --port 9222
```

## Codex Implementer Prompt (Phase별로 N만 바꿔 사용)

```text
/codex:rescue Read docs/plans/backend-visual-map-final-product.md and docs/plans/backend-visual-map-trust-and-clarity.md.
Implement Phase T<N> only with the smallest safe patch. Reuse existing patterns
(run_engine_command*, engine_json_value, object_string, serde(default) for snapshot compat).
Do not render raw full graphs. Do not add row-data access. Do not persist passwords.
Keep name-based code→table links as candidates; only engine-reported FK/CALLS may be confirmed.
Run: npm run typecheck, npm run build, cargo test. Report changed files, checks, skipped work, risks.
```

## Codex Reviewer Prompt

```text
/codex:rescue You did not write this code. Review the current diff only against Phase T<N> in
docs/plans/backend-visual-map-trust-and-clarity.md. Do not modify files. Findings first.
Focus on: candidate links mislabeled as confirmed, snapshot backward-compat breaks (serde default),
engine-call volume/timeouts, node/edge cap violations, secret leakage, phase-scope creep.
```

## Risks

- describe-table 40회 호출로 `테이블 불러오기` 시간이 늘어남 (테이블 20개 기준 수 초 예상) → 진행 상태 문구 유지, 상한 준수.
- DDL/SQLite 어댑터의 describe 출력 스키마가 postgres와 다를 수 있음 → object_string 다중 키 파싱 + 실패 시 기존 경로 fallback (T1에 스모크 포함).
- CALLS total이 큰 저장소 → LIMIT 캡 + 인벤토리 교집합 필터로 스냅샷 팽창 방지.
- 구 스냅샷과의 혼재 → links 없으면 기존 렌더와 동일해야 함 (회귀 테스트).
```
