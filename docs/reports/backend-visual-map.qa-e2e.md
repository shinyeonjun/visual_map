# Backend Visual Map 실사용 E2E QA (전시/시연 기준)

Date: 2026-07-06
기준: "돌아간다"가 아니라 "처음 보는 사람이 이해하고 조작할 수 있다".
방법: 앱 데이터 초기화 후 `npm run tauri dev` 실앱을 WebView2 CDP로 조작하며 사용자 플로우 전체를 실행.

## 테스트 환경

- 앱 데이터 백업: `%APPDATA%\com.backendvisualmap.app.backup-20260706-181225` (초기 상태에서 시작)
- 코드 저장소: `D:\meeting-overlay-assistant`
- DB: Docker `caps-postgresql-dev` (PostgreSQL, `127.0.0.1:55432`), DSN은 세션 입력만 사용
- 엔진: `BACKEND_VISUAL_MAP_ENGINE_DIR`로 저장소 번들 엔진 사용 (문서화된 dev 경로)

## E2E 결과 (수정 후 재실행 포함)

| 단계 | 결과 |
| --- | --- |
| 초기 화면: 워크스페이스 0, 코드 0, DB 소스 없음, 이전 데이터 없음 | PASS |
| 워크스페이스 생성 (`D:/meeting-overlay-assistant`) | PASS |
| 코드 인덱싱 → 코드 불러오기 (라우트 50 / 서비스 80 / 파일 12 표시) | PASS |
| PostgreSQL 메타데이터 연결 테스트 (세션 DSN) | PASS |
| DB 인덱싱 → 테이블 불러오기 (테이블 20 / 컬럼 221 표시) | PASS |
| 중앙 캔버스: 원본 383개 항목 → 그룹 13개, raw graph 미노출, 한 줄 붕괴 없음 | PASS |
| 모드 전환 5종: 아틀라스 13 / API 흐름 21 / 테이블 사용처 10 / 컬럼 영향도 2+요약 / 검색 | PASS (죽은 화면 없음) |
| Workbench ↔ Atlas 왕복 (뷰 스위치 + 로고 클릭 2경로) | PASS |
| 줌: 휠 줌인/아웃, +/-, 100%, 화면 맞춤, pan 모드 스크롤 이동 | PASS |
| 검색: "accounts" → API 1 + 테이블 1 그룹핑 → 선택 시 포커스 맵 | PASS |
| 앱 재시작: 워크스페이스/코드 142/테이블 20/맵 자동 복구, 연결 문자열은 비어 있음 | PASS |
| 보안: 앱 데이터에서 `caps:caps`/`postgresql://`/password/포트 저장 여부 rg 스캔 | PASS (미저장) |
| `npm run typecheck` / `npm run build` / `cargo test`(56) | PASS |

보안 스캔 상세: `graph.sqlite`의 `caps:caps` 일치는 데이터베이스명과 스키마명이 모두 "caps"라서 생긴 메타데이터 복합 키(`...:caps:caps:database:caps`)였고, 엔진 캐시에는 `connection_alias`(프로필 ID)만 저장된다. DSN, 비밀번호, 호스트/포트는 어떤 파일에도 없다. `workspace.json`은 `passwordStored: false`뿐이다.

## 고친 버그

1. **노드 클릭 시 줌/팬 리셋** — 노드를 선택할 때마다 fitView가 재실행되어 사용자의 뷰가 튀었다. 자동 fit을 맵 정체성(id/mode/focus/노드 수) 변경 시로 한정. (`WorkbenchCanvas.tsx`)
2. **레이어 열 겹침** — API 레이어의 서브컬럼이 코드 레이어 시작 x를 침범해 카드가 겹쳐 보였다. 레이어별 실제 컬럼 수 기반 누적 오프셋으로 교체, 겹침 0 확인. (`WorkbenchCanvas.tsx`)
3. **DB 연결 테스트/인덱싱이 표시 중인 맵을 삭제** — 테스트 성공 직후 캔버스가 "맵 데이터 없음"으로 떨어졌다. 목록만 비우고 스냅샷 기반 맵은 유지하도록 수정. 코드 재인덱싱도 동일. (`useDbProfiles.ts`, `useCodeInventory.ts`)
4. **소규모 맵 과확대** — 1~2 노드 맵이 최대 배율(2.4x)로 fit되어 노드 하나가 화면을 채웠다. fit 배율 상한 1.2 적용.
5. **가로로 긴 레이아웃으로 글자가 안 읽힘** — 레이어당 행 수를 늘려 세로 우선 배치, API 흐름 21노드 맵의 fit 배율이 0.34→0.47로 상승.

## 개선한 UX/UI

1. **Atlas 모드 전환이 실제로 화면을 바꿈** — 기존에는 어떤 모드를 눌러도 밴드 3개가 그대로였다("모드 바꿔도 변화 없음" 체감의 원인). 이제 API 모드=라우트+코드, 의존성/스키마=코드+DB, 영향도=DB만 표시하고 밴드 번호도 재계산. 선택된 테이블은 밴드 맨 앞으로 정렬되어 항상 보인다. (`AtlasCanvas.tsx`)
2. **포커스 맵 진입 시 인스펙터 자동 표시** — 컬럼 영향도로 전환해도 인스펙터가 이전 "선택된 코드"를 보여주던 불일치 해소. 포커스 노드를 자동 선택해 영향 요약이 즉시 뜬다. 사용자가 고른 선택은 덮어쓰지 않는다. (`useVisualMap.ts`)
3. **빈 흰 상자 미니맵과 중복 컨트롤 제거** — 렌더되지 않는 MiniMap과 툴바와 중복인 React Flow Controls 제거.
4. **잘린 kind 칩 정리** — "Functi" 같은 잘린 라벨을 ROUTE/FUNC/CLASS/SVC 등 의미 있는 약어로 교체. Workbench 목록·Atlas 트리·Atlas 카드 공통. (`codeKindChip`)
5. **Atlas 범례 정리** — 밴드 뷰에 존재하지 않는 "DB 외래 키/위험·경고" 범례 항목을 실제 동작 설명으로 교체.

## 스크린샷

`docs/reports/screenshots/qa-e2e/`

- `01-initial-empty.png` 초기 빈 화면
- `02-code-loaded.png` 코드 로드 직후 (수정 전 레이아웃 기록용)
- `03-db-loaded.png` DB 로드 후 (수정 전, 겹침 재현 기록)
- `04-atlas.png` Atlas 전체
- `05-atlas-modes.png` Atlas 모드 반응
- `06~10-mode-*.png` Workbench 모드 5종 (수정 후)
- `11-restart-restore.png` 재시작 복구

## 아직 부족한 점

1. **검색 모드 기본 화면이 빈약** — 검색어 없이 모드만 누르면 노드 1개짜리 맵. 검색어 입력을 유도하는 안내가 더 있으면 좋다.
2. **컬럼 영향도의 후보 코드가 자주 0개** — `created_at` 같은 범용 컬럼명은 이름 기반 매칭이 못 잡는다. 엔진의 컬럼 타입/참조 출력이 개선되어야 한다 (`타입 ?` 표기도 동일 원인).
3. **상태 바 우선순위가 최근성 기반이 아님** — DB 작업 직후에도 맵/코드 상태가 상태 바를 차지할 수 있다. 패널 메시지는 정확하므로 시연 blocker는 아님.
4. **Atlas 그룹 드릴다운 미구현** — 그룹 클릭 시 내부 지도로 들어가는 product 약속은 여전히 기능 작업으로 남아 있다.
5. **API 흐름 맵의 코드→DB 연결 근거가 이름 매칭** — 후보(점선)로 정직하게 표시되지만, 실제 호출 추적은 아니다.
6. **THIRD_PARTY_NOTICES 라이선스 고지 미완** — 공개 배포 전 필수 (기존 blocker 유지).

## 전시/시연 가능 여부

**가능.** 초기 상태에서 워크스페이스 생성 → 코드/DB 인덱싱 → 5개 모드 탐색 → 재시작 복구까지 처음 보는 사람 기준의 플로우가 죽은 화면 없이 이어지고, 비밀번호/DSN은 어디에도 저장되지 않는다. 남은 항목은 완성도(검색 기본 화면, 후보 품질)지 동작 결함이 아니다. 단, 배포(설치본 배부)는 라이선스 고지 완료 전까지 하지 말 것.

## 다음 우선순위

1. Atlas 그룹 클릭 드릴다운 (product 약속 이행, backend focus 모델 확장)
2. `database-memory` 컬럼 타입 출력 개선 → `타입 ?` 제거, 컬럼 후보 품질 향상
3. 검색 모드 빈 상태 안내 + 검색창 자동 포커스 연동
4. 상태 바 최근 작업 우선 표시
5. THIRD_PARTY_NOTICES upstream 라이선스 원문 확보
