# 선택 API 중심 실행 흐름 구현 계획

- 상태: 설계 승인 대기
- 연구: `docs/research/focused-api-flow.md`
- 원칙: 기존 snapshot/link/answer 모델을 우선 재사용하고, provider가 증명하지 못한 관계를 UI에서 추측하지 않는다.

## 완료 정의

다음이 모두 되어야 focused API flow를 완성으로 본다.

1. 인덱싱 후 확정 진입점이 있으면 대표 API focused flow를 기본으로 연다.
2. route, middleware, handler, service, repository/query, DB가 확정 관계일 때 한 캔버스에서 분기와 함께 보인다.
3. 각 노드·edge에 source path/line, provider/strategy, truth class, evidence가 있다.
4. provider 누락, 인벤토리 정규화 실패, DI 모호성, ORM 미지원, DB snapshot 부재를 서로 다른 gap으로 보인다.
5. 확정 edge와 candidate/unknown edge가 섞이지 않는다.
6. hop·node·edge cap, cycle, timeout/cancel, stale snapshot이 결과에 반영된다.
7. 이름 변경·파일 이동·동명이인·순서 변경·부분 provider 실패 변형 테스트를 통과한다.
8. Node/Express와 Java/Spring 각각 최소 한 개의 인증 fixture와 실제 공개 저장소 smoke가 통과한다.

## 단계

### Phase 0 — 실제 누락 지점 분해와 계약 기준선

목표는 코드를 바로 늘리는 것이 아니라, `authService`가 왜 화면에서 사라지는지 한 번의 재현으로 분해하는 것이다.

검사 경로:

```text
provider 결과
  -> CodeInventory.calls / handles / relation_gaps
  -> InventorySnapshot.links
  -> api_flow_map traversal
  -> ApiReadingAnswer.steps / unknowns
  -> buildApiConnectionModel
  -> 화면/근거 패널
```

확인 항목:

- `controller -> authService` 호출이 provider JSON에 있는가
- 호출자의 `from`과 대상 `to`가 인벤토리 안정 ID와 일치하는가
- `unresolved-call` gap이 source/line을 보존하는가
- snapshot link가 `confirmed code_call/CALLS`로 저장되는가
- `API_CALL_HOP_LIMIT=4` 또는 node/edge cap 때문에 잘리지 않았는가
- answer steps와 map edges에 동일 edge가 모두 들어가는가

산출물:

- `docs/reports/focused-api-flow.phase-0.md`
- 실제 재현용 최소 fixture
- provider 누락/정규화 누락/투영 누락 중 root cause 하나 이상 확정

검증 게이트: 원인을 모른 채 UI에 임시 edge를 추가하지 않는다.

### Phase 1 — focused flow 진입과 증거/갭 투영

새 parser를 추가하지 않고 현재 결과를 더 정직하고 읽기 좋게 투영한다.

- 대표 API 자동 선택 규칙과 복수 route 선택 상태
- focused flow 기본 진입, 전체 맵 별도 모드
- `ApiReadingAnswer`의 gap에 중단 위치·관계·원인·source evidence 추가
- 모든 추가 분기를 동일 canvas에 표시
- 확정/candidate/unknown/stale/truncated 배지와 inspector 연결
- `DB 없음` 문구를 `DB 관계를 증명하지 못함`과 source scope 안내로 분리

검증 게이트:

- route만 있는 경우 fake handler/service가 생기지 않음
- handler가 있지만 DB snapshot이 없으면 코드 prefix는 유지되고 DB만 gap임
- partial/stale 결과가 complete처럼 보이지 않음
- 기존 153개 프론트 테스트와 Rust 테스트 의미가 깨지지 않음

### Phase 2 — 공통 직접 호출 체인

우선 Node/TypeScript/JavaScript와 Python의 정적 직접 호출부터 한다.

- import/require/module alias 정규화
- caller/callee 안정 ID와 source range
- `CALLS`, `ASYNC_CALLS` 또는 동일 공통 relation 표현
- re-export와 단일 receiver 해석
- cycle/duplicate 제거
- 대상이 없거나 모호하면 unresolved/ambiguous gap

처음 확정할 체인:

```text
Route -> Middleware -> Handler -> Service -> Repository/Query
```

하지 않을 것: 동적 dispatch, reflection, 문자열 기반 target을 확정으로 올리지 않음.

검증 게이트: 같은 이름의 함수가 두 개일 때 확정 edge 수가 늘지 않고 ambiguity gap이 생긴다.

### Phase 3 — framework pack과 DI

현재 실제 검증 수요가 있는 Node/Express와 Java/Spring을 먼저 인증한다.

Node/Express:

- `router.post(path, validate, controller.login)` 배열 순서
- middleware와 최종 handler 구분
- import/require로 controller와 authService의 소유권 확인
- `module.exports`, ESM import, re-export의 중복 제거

Java/Spring:

- `@RequestMapping`/`@GetMapping` 등 controller entry
- constructor/field injection
- `@Service`, `@Repository`, interface와 유일 구현체
- controller→service→repository 호출
- overload/모호한 bean은 candidate/unknown

검증 게이트: route가 여러 handler로 연결될 수 있을 때 하나를 조용히 확정하지 않는다.

### Phase 4 — ORM/DB exact linking

- provider가 SQL/ORM/query와 source evidence를 만든다.
- DB engine이 table/schema/column/alias snapshot을 제공한다.
- linker가 정확히 조인할 때만 `READS`/`WRITES` 확정 edge를 만든다.
- 후보 테이블은 candidate로 유지하고, DB snapshot 부재는 capability gap으로 표시한다.

검증 게이트:

- `User` 클래스명만으로 `users` table이 생기지 않음
- 동일 이름 table이 다른 schema에 있으면 ambiguity
- DB snapshot stale/truncated면 확정 관계를 complete로 표시하지 않음

### Phase 5 — UI 읽기성·분기·전체 맵 분리

- focused flow는 기본 진입 화면
- global map은 탐색 모드
- 분기는 한 canvas에서 fan-out으로 표시
- 노드 선택 시 source/line/근거/지원등급 표시
- branch가 많으면 node-level collapse와 표시 cap을 사용하되 숨긴 수와 이유 표시
- 모바일/작은 창/긴 경로/한국어 라벨에서 footer와 canvas가 겹치지 않음

검증 게이트: 1개, 10개, 100개 branch에서 선·노드·근거 선택이 서로 어긋나지 않는다.

### Phase 6 — 살충제 패러독스·성능·출시

- fixture/property/metamorphic/negative/differential 테스트를 CI에 추가
- node-express-boilerplate와 spring-petclinic-microservices smoke
- 혼합 언어와 monorepo 샘플
- source revision/stale 캐시 검증
- 파일·심볼·edge 수별 시간·메모리·취소 검증
- 설치 후 Windows smoke에서 번들 provider와 engine을 실제로 확인

출시 게이트: 한 샘플 화면이 예쁘다는 이유가 아니라, 인증 capability의 정밀도·갭 설명·중복률·성능 상한을 통과해야 한다.

## 단계별 변경 경계

| 단계 | 우선 변경 위치 | 건드리지 않을 것 |
| --- | --- | --- |
| 0 | fixture, trace/report, 테스트 | 화면 임시 edge |
| 1 | Rust answer/model + API flow UI 투영 | 신규 언어 parser |
| 2 | provider 결과 정규화, code inventory | framework별 UI |
| 3 | framework pack과 DI linker | 이름 휴리스틱 확정 |
| 4 | query evidence/DB linker | row data 저장·런타임 실행 |
| 5 | focused/global layout와 evidence panel | 전역 그래프 무제한 표시 |
| 6 | CI/fixture/실프로젝트/패키징 | 인증되지 않은 지원 선언 |

## 위험과 대응

| 위험 | 대응 |
| --- | --- |
| provider가 호출을 찾지 못함 | provider capability와 gap을 보존하고 pack 규칙으로 덮지 않음 |
| 호출 대상이 inventory에 없음 | unresolved symbol을 버리지 말고 source/line gap으로 투영 |
| DI가 런타임에 결정됨 | prefix 확정 + 다음 지점 candidate/unknown |
| UI가 추가 edge를 중복 표시 | stable edge ID와 primary/additional 공통 dedupe |
| 대형 repo에서 폭발 | bounded traversal, node/edge cap, cycle, cancel |
| 공개 저장소 특례 증가 | 모든 규칙에 positive/negative/metamorphic fixture 요구 |
| DB 오탐 | exact snapshot join 없이는 confirmed 금지 |

## 권장 첫 구현 순서

1. Phase 0에서 node-express-boilerplate의 실제 `controller -> authService` 누락 원인을 확정한다.
2. 결과를 source/line 포함 gap으로 보여주는 계약을 먼저 만든다.
3. provider가 직접 호출을 안정 ID로 내보내면 Phase 2의 공통 traversal로 연결한다.
4. Node/Express 한 stack에서 `Route -> Middleware -> Handler -> Service`를 인증한다.
5. 같은 계약으로 Java/Spring `Controller -> Service -> Repository`를 인증한다.
6. 그 다음에 ORM/DB 확정과 다른 언어를 확장한다.

첫 구현 목표는 “모든 언어의 완성 흐름”이 아니라 “두 stack에서 확정 prefix와 끊긴 이유가 모두 신뢰성 있게 보이는 것”이다. 이 기준을 통과해야 범위를 넓힌다.
