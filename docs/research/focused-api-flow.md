# 선택 API 중심 실행 흐름 연구

- 상태: 제안
- 작성일: 2026-08-01
- 범위: 인덱싱 완료 후 선택 API의 `Route -> Middleware -> Handler -> Service -> Repository/Query -> DB` 흐름
- 전제: 확정 관계와 후보/미확인 관계를 절대 같은 의미로 표시하지 않는다.

## 1. 결론

기술적으로 충분히 가능하다. 현재 제품은 이미 이 기능의 뼈대를 가지고 있다.

- `api-flow`가 route를 시작점으로 삼는다.
- 확정 `HANDLES`와 확정 `CALLS`를 bounded traversal한다.
- `ApiReadingAnswer.steps`에 깊이·레이어·근거를 저장한다.
- `VisualMap.edges`에 확정 코드 관계와 DB 관계를 투영한다.
- DB는 정확한 snapshot 조인 전에는 확정하지 않고 candidate/unknown으로 남긴다.
- UI는 확정 경로와 추가 분기를 같은 캔버스에서 선택할 수 있다.

따라서 새 제품을 처음부터 만드는 문제는 아니다. 다만 현재 구현은 “선택 API에서 증명된 코드 관계를 읽기 좋게 보여주는 1차 답변”에 가깝다. `controller -> authService -> User/Token -> DB`를 범용적으로 보장하려면 provider, framework pack, DB linker, API flow 투영을 순서대로 보강해야 한다.

## 2. 현재 코드에서 확인한 사실

### 2.1 이미 있는 흐름

`src-tauri/src/atlas/api_flow.rs`의 현재 경로는 다음과 같다.

```text
선택 route
  -> 확정 code_handle/HANDLES
  -> 확정 code_call/CALLS 최대 4 hop
  -> 확정 code_db_read/code_db_write
  -> candidate DB link
  -> ApiReadingAnswer와 VisualMap으로 투영
```

현재 `trusted_api_edge`는 `truthClass=confirmed`, 예상 `kind`, 예상 `engineEdgeType`을 모두 요구한다. 이는 오탐 방지에는 올바르지만, provider가 `CALLS`를 만들지 못하거나 대상 심볼이 인벤토리에 없으면 뒤 구간이 보이지 않는다.

`src-tauri/src/workspace/code.rs`의 `extract_code_calls_with_gaps`는 호출 양 끝점이 인벤토리에 없을 때 관계를 억지로 확정하지 않고 `unresolved-call` gap으로 보낸다. 이 역시 정책상 맞지만, 현재 UI에서 이 gap이 “어느 파일의 어느 호출에서 끊겼는지” 충분히 크게 보이지 않으면 사용자는 분석기가 호출을 누락했다고 느낀다.

`src/components/atlas/apiConnectionModel.ts`는 `ApiReadingAnswer.steps`와 `VisualMap.edges`에서 주 경로를 고른다. 즉 provider/engine에 관계가 있어도 `steps`와 `map.edges`에 함께 들어오지 않으면 UI는 그 관계를 흐름으로 그릴 수 없다.

`src/components/atlas/SetupChecklist.tsx`에는 기본 route를 골라 `api-flow`로 진입하는 경로가 이미 있다. 따라서 “전체 맵을 먼저 보여주지 말고 선택 API focused flow를 기본 화면으로 하자”는 제안은 새 UX 원칙으로 정리하면 되며, 기본 route 선택·복수 route 선택·자동 진입 시점만 다듬으면 된다.

### 2.2 현재 화면에 DB가 안 나오는 것이 의미하는 것

화면의 `DB 근거 못 찾음`은 DB가 반드시 없는다는 뜻이 아니다. 현재 계약상 다음을 구분해야 한다.

| 상태 | 실제 의미 | UI 표현 |
| --- | --- | --- |
| 확정 DB 관계 없음 + 후보 없음 | 확정된 코드 경로에서 DB 사용을 증명하지 못함 | `DB 사용 구간을 확인할 수 없음` + 끊긴 지점 |
| 코드 호출이 인벤토리에 없음 | provider가 호출을 냈지만 대상 정규화가 실패함 | `unresolved-call` + source/line |
| 호출 자체가 없음 | provider/parser가 호출을 발견하지 못함 | provider capability/gap |
| ORM/DI가 모호함 | 여러 구현 또는 런타임 바인딩 가능 | candidate/unknown |
| DB snapshot이 없음 | 코드 쪽 증거는 있어도 DB 조인이 불가능함 | `DB 구조 연결 필요` |
| snapshot이 stale/truncated | 결과 범위가 현재 소스를 완전히 대표하지 않음 | stale/partial + 재인덱싱 안내 |

이 분리를 하지 않고 `DB 0`만 보여주면 사용자는 “DB 분석 실패”와 “정말 DB를 안 씀”을 구별할 수 없다.

## 3. 얼마나 어려운가

### 3.1 가능한 범위

다음은 충분히 현실적인 1차 목표다.

```text
Route
  -> route 등록 미들웨어/검증
  -> 실제 handler/controller
  -> 정적으로 확인된 직접 호출
  -> service
  -> repository/query
  -> 정확한 DB snapshot 매칭
```

단, 첫 버전의 확정 조건은 보수적으로 잡아야 한다.

- import/require/module 경로와 심볼이 확인됨
- 호출자의 source range와 호출 표현식이 확인됨
- 수신자 타입 또는 유일한 심볼 대상이 확인됨
- framework 등록부에서 handler 소유권이 확인됨
- DI가 constructor/annotation/명시적 provider 등으로 정적으로 확인됨
- SQL/ORM query가 DB 인벤토리와 정확히 매칭됨

### 3.2 어려운 범위

다음은 언어 공통 규칙으로 해결하면 안 된다.

- JavaScript의 동적 property 호출, 문자열 기반 `require`, 런타임 monkey patch
- Java/Spring의 여러 bean 구현체, profile/conditional bean, reflection, generated proxy
- Kotlin/Java overload와 interface dispatch
- Python의 동적 import, decorator factory, monkey patch
- ORM이 query를 실행 시점에 조립하는 경우
- 코드 생성 파일과 실제 원본의 소스 위치 연결
- reflection, dependency injection container, plugin registry를 실행 없이 100% 해석

이 경우에도 제품이 무력해지는 것은 아니다. 확인된 prefix까지는 확정 흐름으로 보여주고, 끊긴 정확한 지점에 candidate/unknown과 근거를 남기면 된다.

### 3.3 상대적 난이도

| 단계 | 핵심 작업 | 난이도 |
| --- | --- | --- |
| focused flow 기본 진입·분기 표시 | 기존 answer/map 재사용, UX 투영 | 중 |
| Node/TS·Python 직접 호출 체인 | import/alias/receiver와 안정 ID | 중상 |
| middleware 순서·handler ownership | framework pack 규칙과 중복 제거 | 중상 |
| Java/Spring DI·controller/service/repository | 타입·annotation·bean 바인딩 | 상 |
| ORM/query -> table/column 확정 | query evidence + DB exact join | 상 |
| 14개 언어 전부 같은 품질 | 각 provider/pack/fixture 지속 운영 | 매우 상, 지속 작업 |

“모든 언어에서 모든 호출을 완벽히 연결”은 일회성 기능 목표로 약속하면 안 된다. “지원 등급별로 어디까지 확정하는지”를 제품 계약으로 약속하는 것이 맞다.

## 4. 권장 제품 동작

### 4.1 전체 맵과 focused flow 역할 분리

```text
인덱싱 완료
  |
  +-- 확정 진입점 1개 --------> 해당 API focused flow를 기본 진입 화면으로 표시
  |
  +-- 확정 진입점 여러 개 ----> 대표 API를 자동 선택하되 선택 이유와 전환 목록 표시
  |
  +-- 확정 진입점 없음 --------> 구조 탐색 + 후보/gap 화면. 가짜 API flow 생성 금지
  |
  +-- 전체 맵 -----------------> 별도 탐색 모드. 프로젝트 전체 관계와 규모 확인
```

focused flow는 읽기와 설득용이고, 전체 맵은 탐색용이다. 전체 맵에서 노드를 작게 축소해 모든 것을 한 번에 읽히게 만들려는 목표는 버린다.

### 4.2 흐름 표시 규칙

- 기본 깊이: `Route(0) -> Middleware(1) -> Handler(2) -> Service(3) -> Repository/Query(4) -> DB(5)`
- 기본은 4~6 hop 안에서 멈추고, 노드·관계 상한을 넘으면 `truncated`를 표시한다.
- 모든 분기는 같은 캔버스에 그리되, 한 노드에서 갈라지는 fan-out은 branch로 보존한다.
- 순환 호출은 방문 ID로 차단하고 `cycle` 표시를 남긴다.
- 확정 edge와 candidate edge는 선·색·배지·근거 패널에서 구별한다.
- `DB 없음`이라는 단정 대신 `현재 확정 경로에서 DB 관계를 증명하지 못함`이라고 표시한다.
- 각 끊김에는 `provider가 못 냈는지`, `정규화에서 잃었는지`, `DI/ORM이 모호한지`, `DB snapshot이 없는지`가 있어야 한다.

### 4.3 대표 API 자동 선택

자동 선택은 반드시 확정 근거를 기준으로 한다.

1. 확정 `HANDLES`가 있고 확정 `CALLS`가 가장 긴 route
2. 확정 `HANDLES`가 있고 최근/대표성 점수가 높은 route
3. 확정 route가 하나뿐인 경우 그 route
4. 그 외에는 사용자가 선택

동률이면 조용히 하나를 고르지 않고 “대표 API 선택” 상태를 보여준다. 테스트 route, fixture route, generated route는 별도 필터 또는 낮은 우선순위로 다룬다.

## 5. 권장 기술 해법

### 5.1 새 언어별 UI가 아니라 공통 flow 계약

기존 `CodeInventory`, `SnapshotLink`, `ApiReadingAnswer`, `VisualMap`을 먼저 재사용한다. 새 타입을 만들더라도 다음 의미를 기존 계약에 맞춰 확장하는 정도로 제한한다.

```text
FlowRun
  snapshotId/sourceRevision
  entryPoint
  nodes[]
  edges[]
  gaps[]
  capabilities[]
  quality: complete | partial | failed | stale | unsupported

FlowNode
  stableId, kind, lane, sourceLocation

FlowEdge
  from, to, relation
  truthClass, confidence
  provider, strategy
  evidence[]

FlowGap
  from, expectedRelation, reason
  candidates[], evidence[]
```

핵심은 `FlowGraph`라는 새 병렬 모델을 무조건 도입하는 것이 아니다. 먼저 현재 `SnapshotLink`와 `ApiReadingAnswer`가 이 필드를 잃지 않고 전달하는지 확인하고, 부족한 필드만 최소 확장한다.

### 5.2 provider의 책임

provider는 언어를 이해하고 다음을 내보내야 한다.

- 정의 심볼의 안정 ID
- 호출자의 안정 ID와 호출 대상의 안정 ID 또는 unresolved symbol
- import/require/module 경로
- 호출 표현식과 source range
- receiver/type/overload 정보가 있으면 함께 기록
- confidence와 사용 전략
- 대상이 인벤토리에 없을 때도 gap 원인을 잃지 않음

이름 문자열만 보내면 공통 adapter가 책임질 수 없다. `authService.loginUserWithEmailAndPassword()`가 보이더라도 같은 이름의 함수가 여러 개면 확정하면 안 된다.

### 5.3 framework pack의 책임

공통 adapter는 결과를 정규화하고, framework pack은 프레임워크의 의미를 해석한다.

| 팩 | 1차 확정 대상 |
| --- | --- |
| Express/Fastify/Nest | route method/path, middleware 배열 순서, handler ownership, import 기반 controller 호출 |
| FastAPI/Flask/Django | decorator/router, dependency/middleware, handler ownership |
| Spring MVC/WebFlux | mapping annotation, controller method, constructor/field DI, service/repository 호출 |
| Spring Data/JPA | repository method, JPQL/native SQL/query annotation과 DB snapshot 매칭 |

팩은 “이름이 controller처럼 생겼다”가 아니라 등록 API·annotation·import/type·source range의 조합으로만 확정해야 한다.

### 5.4 traversal/linker의 책임

공통 traversal은 다음만 한다.

- 확정 edge를 bounded BFS/DFS로 따라감
- 노드·edge 중복 제거
- cycle과 branch 보존
- cap/timeout/cancel 처리
- 후보/unknown/gap을 끊긴 위치에 연결
- 같은 의미의 `CALLS`와 `code_call`을 중복 표시하지 않음

traversal이 이름으로 새로운 관계를 만들면 안 된다. 관계 생성은 provider/framework/DB linker의 책임이다.

### 5.5 DB 관계의 책임

다음 2단계를 분리한다.

```text
코드 provider: SQL/ORM/query/DbReference 증거 생성
        |
        v
DB linker: database-memory snapshot과 정확히 조인
        |
        v
확정 READS/WRITES table/column
```

코드에 `User`나 `users`라는 단어가 있다는 이유만으로 테이블 관계를 확정하지 않는다. snapshot이 없거나 schema/table/alias가 모호하면 candidate/unknown으로 남긴다.

## 6. 살충제 패러독스를 막는 검증 설계

한 공개 저장소의 화면 스냅샷만 회귀 테스트로 삼으면 다시 같은 프로젝트에 과적합된다. 다음을 모두 사용해야 한다.

### 6.1 단위·계약 테스트

- 직접 cross-file 호출
- import alias와 re-export
- 같은 이름의 동명이인
- method receiver와 overload
- async/await, callback, generator
- middleware 배열 순서
- constructor/annotation DI
- repository method와 SQL/ORM query
- unresolved endpoint가 gap으로 보존되는지

### 6.2 부정 테스트

- 문자열/주석에만 route 또는 함수명이 존재
- 동적 `require`/reflection
- 두 bean 구현체가 동시에 존재
- generated/test/fixture 파일이 운영 흐름으로 승격
- DB snapshot 없음·stale·truncated
- provider 일부 실패
- 같은 edge가 여러 provider에서 중복 보고됨

### 6.3 변형(metamorphic) 테스트

관계의 의미는 유지하고 표면만 바꾼다.

- 함수·파일·폴더 이름 변경
- 파일 이동과 import 순서 변경
- 선언 순서 변경
- unrelated 동명이인 추가
- 하나의 handler에 두 service 분기 추가
- route method/path 변경
- 유사하지만 다른 framework 등록 코드 추가
- 한 provider 결과만 지연/실패

변형 후 기대할 것은 “특정 이름이 나온다”가 아니라 안정 ID의 의미 관계, 확정/후보 분류, gap 사유, 중복 수가 올바른지다.

### 6.4 실제 프로젝트 매트릭스

최초 인증 세트는 다음 세 축으로 잡는 것이 좋다.

1. `node-express-boilerplate`: Node/Express, middleware/controller/service 호출과 중복 route 방지
2. `spring-petclinic-microservices`: Java/Spring, gateway/controller/service/repository 및 다중 서비스 경계
3. 작은 독립 fixture: ambiguity·DI·ORM·실패 상황을 의도적으로 재현

Petclinic과 node-express는 smoke 검증용이고, 모든 규칙의 정답 데이터가 아니다. 실제 저장소와 작은 fixture의 결과가 모두 맞아야 한다.

## 7. 성공 지표

recall 하나만 높이면 오탐이 늘어 살충제 패러독스가 생긴다. 다음을 함께 본다.

- confirmed edge precision: 확정이라고 한 관계 중 실제 정답 비율
- certified fixture recall: 인증한 패턴 안에서 놓친 관계 비율
- path completion: entry에서 repository/query까지 도달한 인증 흐름 비율
- gap reason coverage: 끊긴 모든 관계에 원인이 있는 비율
- duplicate edge rate: 같은 의미 관계가 중복 표시되는 비율
- stale safety: 소스 변경 뒤 이전 확정 관계가 재사용되지 않는 비율
- performance: 파일·심볼·관계 수별 시간/메모리와 cap 동작

“전체 프로젝트의 모든 관계를 찾았다”는 측정 목표로 삼지 않는다. 정밀 지원이라고 선언한 capability의 품질을 측정한다.

## 8. 하지 말아야 할 것

- 모든 언어를 한 번에 완전 연결한다고 약속
- 함수명·폴더명·테이블명 일치만으로 확정
- UI에서 끊긴 관계를 임의의 점선으로 만들어 성공처럼 보임
- 현재 `map.edges`만 늘려서 provider의 누락을 덮음
- 한 프로젝트의 특이한 폴더나 함수명에 전용 예외 추가
- 런타임 실행을 기본 분석으로 넣어 보안·재현성 문제를 키움

## 9. 최종 판단

이 기능은 빡세지만 불가능한 기능은 아니다. 현실적인 제품 약속은 다음이다.

> 지원 등급이 선언된 언어·framework·ORM 조합에서는 선택 API의 확정 prefix를 파일·라인·전략 근거와 함께 보여주고, 분석할 수 없는 다음 구간은 정확한 gap으로 설명한다.

이 약속이면 Node/Express와 Python/FastAPI에서 먼저 높은 완성도를 만들고, Java/Spring은 controller→service→repository부터 인증한 뒤 DI·ORM·DB 범위를 단계적으로 늘릴 수 있다. 이것이 범용성, 신뢰성, 개발 속도를 동시에 지키는 경로다.
