# 다언어 클라이언트 요청 연결 구현 계획

- 상태: Phase 1 완료, Phase 3 focused flow 일부 완료
- 연구: `docs/research/client-request-reliability.md`
- 목표: 가능한 클라이언트 HTTP 요청은 자동 연결하고, 불확실한 요청은 공통 상태와 근거로 정직하게 표시한다.

## 완료 정의

1. `ClientRequest`가 모든 언어에서 같은 필드·상태·stable ID를 사용한다.
2. 정적으로 유일하게 확인된 요청만 서버 route와 confirmed link가 된다.
3. 동적 URL·동명이인·지원 밖 framework·test-only는 candidate/unknown/excluded로 남는다.
4. request와 route의 파일·라인·method·path 근거를 UI까지 보존한다.
5. 기존 `CALLS` 의미를 변경하지 않는다.
6. 12개 active 언어에 같은 의미의 positive/negative/metamorphic fixture가 있다.

## Phase 1 — 공통 IR과 정적 literal 추출 — 완료

- Tauri code inventory에 optional `clientRequests`를 추가한다.
- 언어별 UI 분기 없이 source extension과 공통 호출 패턴을 정규화한다.
- literal URL, 안전한 local constant, 단순 template만 추출한다.
- 주석·test-only·생성물은 별도 상태로 분류한다.
- 서버 route canonical matching과 request link를 추가한다.
- unresolved URL은 link를 만들지 않고 gap/evidence로 남긴다.

검증: Rust unit/contract tests, 12개 언어 fixture, 기존 전체 회귀.

실제 완료:

- ClientRequest IR, stable ID, source line, caller, evidence를 추가했다.
- 겹침 윈도우의 실제 호출 줄을 보존해 중복을 제거했다.
- 12개 active 언어 확장자 공통 conformance와 test-only/문자열 fake/dynamic URL
  negative fixture를 추가했다.
- method/path가 유일하게 맞는 route만 CLIENT_REQUEST 확정 관계로 만들고,
  dynamic route와 generic client는 candidate로 보존한다.
- code adapter version을 7로 올려 이전 snapshot을 재색인한다.

## Phase 2 — 설정·prefix·generated client

- `.env`·지원 설정 파일의 non-secret URL 상수 해석
- router mount/prefix canonicalization
- OpenAPI/generated client의 source mapping
- 다중 일치와 stale config의 candidate 처리

## Phase 3 — UI와 focused flow — 일부 완료

- Backend API와 Client Request를 별도 lane으로 표시
- `REQUESTS` edge에 static/runtime 근거 표시
- 확정 경로와 후보/unknown을 분리
- 현재 focused graph의 edge cap/접힘 정책 재사용

실제 완료:

- focused API flow에 최대 4개의 Client Request incoming lane을 추가했다.
- 확정/후보 edge 스타일과 request source/evidence를 유지한다.

남은 작업:

- 4개 초과 요청의 접기 목록과 unknown request를 같은 화면에 표시
- client request 전용 검색/Code panel projection

## Phase 4 — 선택적 runtime observation

- 사용자 명시 실행 또는 격리된 runner에서만 관찰
- secret/body/header 비저장·redaction
- static evidence와 runtime evidence를 별도로 병합
- 실행되지 않은 경로를 부재로 판단하지 않음

## Release gate

- common IR schema test 통과
- language semantic gate와 request conformance gate 모두 통과
- 하나라도 확정 기준을 위반하면 해당 capability만 `partial` 또는 `releaseReady=false`
- “모든 프론트 호출을 서버 API에 연결한다”는 문구는 Phase 2/3 범위를 통과한 뒤에만 사용

## 비목표

- reflection·완전 동적 dispatch를 정적으로 확정하지 않는다.
- 기존 `CALLS`를 client request 의미로 재사용하지 않는다.
- 각 언어마다 별도 truth model이나 UI를 만들지 않는다.
- 기본 인덱싱에서 임의 프로젝트 코드를 실행하지 않는다.
