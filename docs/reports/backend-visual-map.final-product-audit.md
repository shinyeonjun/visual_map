# Backend Visual Map Final Product Audit

> Historical audit from 2026-07-06. Its release blockers were superseded by `backend-visual-map-production-completion.phase-11.md`: the product now uses MIT, third-party notices pass, and the database engine is published and pinned at `v0.1.1`. Trusted Windows code signing remains the only external release gate.

Date: 2026-07-06
Scope: `D:\project\backend_map` 전체를 최종 제품 기준으로 감사하고, 릴리즈 blocker/버그/완성도 문제를 수정.

기준 문서:

- `docs/plans/backend-visual-map-final-product.md`
- `docs/plans/backend-visual-map-product-completion.md`
- `docs/reports/backend-visual-map.phase-75.md`

## Fixed

### 버그 수정

- `src/hooks/useDbProfiles.ts`
  - DB 프로필이 없는 워크스페이스로 전환할 때 이전 워크스페이스의 프로필 이름/소스/경로가 폼에 남던 문제 수정. 이제 프로필이 없으면 폼을 기본값으로 초기화한다.
  - 워크스페이스 전환 시 이전 워크스페이스의 DB 성공/오류 상태 메시지가 상태 바와 패널에 남던 문제 수정.
- `src/hooks/useCodeInventory.ts`
  - 워크스페이스 전환 시 이전 워크스페이스의 코드 상태/오류 메시지가 남던 문제 수정.
  - `indexCodeRepository`의 중복된 `if (result.run.ok)` 블록 병합 (동작 동일, 코드 정리).
- `src/hooks/useVisualMap.ts`
  - 스냅샷 파일이 없을 때 `os error 2`(파일 없음)가 "맵 생성 실패" 오류로 표시되던 문제 수정. 기존에는 `os error 3`(경로 없음)만 "인벤토리 스냅샷 없음"으로 처리했다. scan 이벤트 등으로 `atlas` 디렉터리만 먼저 생기면 error 2가 발생할 수 있다.
- `src/components/WorkbenchCanvas.tsx`
  - 범례의 "점선 = 외래 키" 표기가 실제 렌더링과 달랐던 문제 수정. 점선/애니메이션 엣지는 후보(candidate) 관계다. 범례를 "코드 흐름 / 포함 관계 / 후보 관계 (추정)"로 정정. (Data Trust Rule: 후보 링크를 확정처럼 표기하지 않는다.)
- `src/styles/layout.css`
  - 1440x900에서 상단 "작업대/아틀라스" 뷰 전환 버튼 텍스트가 두 줄로 꺾여 잘리던 시각 결함 수정 (`white-space: nowrap`, `flex: 0 0 auto`).

### UI/UX 완성도

- Ctrl+K 검색 단축키 구현: 두 톱바 모두 placeholder/kbd로 Ctrl+K를 안내했지만 실제 핸들러가 없었다. `src/hooks/useSearchHotkey.ts` 추가, Workbench/Atlas 톱바 검색 입력에 연결.
- 가짜 필터 UI를 실제 동작하는 필터로 교체:
  - `CodeSourceSection` 코드 인벤토리 필터 (이름/경로).
  - `DatabaseSourceSection` 테이블 필터.
  - `AtlasRepositoryPanel` 파일/심볼 필터 (라우트·코드·파일 공통).
  - `AtlasDatabasePanel` 테이블 검색.
  - 목록이 8~10개로 잘리는 기존 구조에서 그 뒤 항목에 접근할 방법이 없었는데, 필터로 도달 가능해졌다. 잘린 개수 힌트("+N개 더 · 필터로 좁히세요")와 "일치 항목 없음" 상태도 추가.
- `AtlasDatabasePanel`의 하드코딩된 스키마 라벨 `public` 제거. 실제 인벤토리의 스키마 이름(없으면 "기본 스키마")을 표시.
- 클릭해도 아무 동작이 없던 인터랙티브 요소 정리 (AtlasTopBar의 기존 `aria-hidden` span 패턴으로 통일):
  - WorkbenchTopBar의 터미널/설정/문서 버튼 → 장식 span.
  - WorkbenchRail 내비게이션 버튼들 → 장식 span (`aria-hidden`).
  - AtlasCanvas의 zoom/잠금/플로팅 도구 버튼들 → 장식 span.
  - WorkbenchCanvas의 가짜 탭 닫기(×)/추가(+) 버튼 제거, 탭은 정적 라벨로 변경.
  - WorkbenchCanvas의 "자동 배치" 버튼 제거 ("화면에 맞춤"과 완전 중복 동작이었음).
  - "100%" 버튼이 실제로 100% 배율(zoomTo(1))로 동작하도록 수정 (기존에는 fitView 중복).
- MiniMap이 빈 흰 상자처럼 보이던 문제 완화 (node/mask 색 지정).

### Codex Follow-up Review

- `src/hooks/useVisualMap.ts`
  - 비주얼 맵 상태를 비울 때 이전 워크스페이스의 맵 성공 메시지와 스냅샷 시간이 남을 수 있던 문제 수정.
- `src/components/atlas/AtlasRepositoryPanel.tsx`
  - Atlas 저장소 패널에도 잘린 개수 힌트와 빈 필터 결과 상태를 추가해 Workbench 필터 UX와 맞춤.
- `src/components/workbench/DatabaseSourceSection.tsx`, `src/styles/forms.css`
  - 단일 "테이블" 탭을 동작 없는 버튼에서 정적 탭 라벨로 변경.

### 문서

- 이 보고서 작성. 스크린샷: `docs/reports/screenshots/final-audit/workbench-loaded.png` (1440x900).
- README/troubleshooting/security-privacy 문서는 감사 결과 현재 동작과 일치하여 수정하지 않았다.

## Checks

- PASS: `npm run typecheck`
- PASS: `npm run build`
- PASS: `cargo test` — 55 passed, 0 failed (Rust 코드 변경 없음)
- PASS: `npm run tauri dev` startup smoke (WebView2 CDP, `scripts/tauri-cdp-smoke.mjs`)
  - 앱 기동, phase71 워크스페이스 자동 복구 (코드 142개, 테이블 18개 스냅샷 복구)
  - Ctrl+K → 검색 입력 포커스 확인
  - 코드 필터 "session" → 라우트 목록 필터링 확인
  - Workbench ↔ Atlas 왕복 내비게이션, 상태 유지 확인
  - 아틀라스 그룹 맵: 원본 334개 항목 → 그룹 14개 노드 (raw graph 미렌더링 확인)
  - 테이블 클릭 → table-usage 맵 (10 노드), 컬럼 클릭 → column-impact 맵 (경고 문구 포함), 아틀라스 복귀 확인
  - 워크스페이스 전환 → 상태 메시지 초기화 후 새 워크스페이스 스냅샷 복구 확인
- PASS: 실 PostgreSQL smoke — `caps-postgresql-dev` (`pgvector/pgvector:pg16`, `127.0.0.1:55432`) 대상으로 `scripts/smoke-rdb-productization.ps1` 재실행 통과
- SKIP: 설치본(NSIS) 재빌드/재설치 smoke — 이번 감사 범위에서 코드 서명·패키징 변경 없음

## Guardrails 확인

- DB row data 접근 추가 없음.
- DB 비밀번호 저장 없음 (연결 문자열은 세션 입력 유지).
- MCP 자동 등록 없음 (`validate_sidecar_args` 차단 로직 유지).
- raw full graph 직접 렌더링 없음 (그룹/포커스 맵 + 렌더 캡 유지).
- 후보 링크 confidence/evidence 표기 유지, 범례는 오히려 더 정확해짐.
- 새 dependency 추가 없음. deferred-after-v1 기능 구현 없음.

## Not Fixed (Blockers 잔존)

1. **Third-party license 고지 미완** — `THIRD_PARTY_NOTICES.md`는 여전히 upstream 엔진 라이선스 원문/저작권 고지가 필요하다고 명시한다. 엔진 upstream 라이선스 정보 없이 앱 쪽에서 임의로 채울 수 없다. **공개 재배포 전 필수.**

## Resolved After Audit

- PostgreSQL 실환경 smoke는 후속 확인에서 `BACKEND_MAP_TEST_POSTGRES_URL=<redacted>`로 재실행해 통과. 더 이상 blocker가 아니다.

## Known Issues (Non-blocking, 미수정)

- Atlas 그룹 노드 클릭 시 그룹 내부로 드릴다운하는 동작(final-product 문서의 "그룹 클릭 시 해당 그룹 내부 지도" 약속)은 미구현. 그룹 클릭은 인스펙터 선택만 갱신한다. 백엔드 focus 모델 확장이 필요한 기능 작업이라 이번 감사 범위(대형 기능 금지)에서 제외.
- `database-memory find-column` 출력에 컬럼 타입이 없어 일부 셀이 `타입 ?`로 표시 (엔진 출력 한계, phase 75 기재 사항 유지).
- `database-memory.exe --version` 미지원 → 엔진 버전 `unknown` (기존 기재 사항).
- `append_scan_event` Tauri command는 프론트엔드에서 호출되지 않는 미사용 경로다. 저장 전 redaction이 있어 위험은 없으며, 커맨드 표면 축소는 다음 정리 기회에 검토.
- 손상된 `workspace.json`이 하나라도 있으면 `list_workspaces` 전체가 실패한다. 이는 ID 변조 감지 테스트로 고정된 의도적 fail-loud 동작이라 유지했으나, 실사용에서 복구 UX가 나쁠 수 있다.
- GitHub URL 검증 로직이 `App.tsx`, `useWorkspaces.ts`, Rust `store.rs` 세 곳에 중복되어 있다 (현재는 동일 동작, drift 위험만 존재).

## Release 판단

**cut scope 후 ship 가능 / 공개 재배포는 hold** — PostgreSQL blocker는 해소됐고, phase 75 판단 중 라이선스 blocker만 유지.

- 다음 범위로는 지금 출시 가능: Windows 로컬 데스크톱, 로컬 코드 인덱싱, SQLite/SQLite DDL/PostgreSQL 메타데이터, 번들 엔진 데모/비프로덕션 사용. 단, 이 축소 범위라도 **공개 배포라면 blocker 1(라이선스 고지)은 먼저 해결해야 한다.**
- 문서화된 전체 공개 배포는 라이선스 고지 완료까지 hold.
