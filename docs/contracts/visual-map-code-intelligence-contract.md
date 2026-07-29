# Visual Map 코드 지능 계약

상태: 기준 문서 1.2

이 문서는 Visual Map이 제공해야 하는 제품 가치를 코드 엔진의 출력 계약으로 고정한다.

1. **처음 보는 백엔드 프로젝트를 빠르게 이해한다.**
2. **코드나 DB 구조를 바꿀 때 영향 범위를 빠르게 확인한다.**

## Visual Map의 제품 정의

Visual Map은 클래스 다이어그램이나 패키지 다이어그램을 만드는 도구가 아니다.
개발자가 기능 하나를 선택했을 때, 그 기능의 실제 동작을 바로 이해하고 수정할 수
있도록 보여주는 **기능 중심 실행 흐름 다이어그램**이다.

기능을 선택하면 다음 흐름을 한 화면에서 읽을 수 있어야 한다.

```text
기능/API
  -> 입력값
  -> 인증·검증
  -> 처리 코드
  -> DB 읽기/쓰기
  -> 외부 서비스·이벤트
  -> 성공·실패 결과
```

예를 들어 로그인 기능은 다음처럼 보여야 한다.

```text
POST /auth/login
  입력: email, password
    -> LoginHandler
    -> 입력값·비밀번호 검증
    -> users 테이블 조회
    -> TokenService
    -> refresh_tokens 저장
    -> 로그인 성공 응답
```

이 정의에서 클래스, 패키지, 모듈은 목적이 아니라 흐름을 설명하기 위해 필요한
경우에만 보조 정보로 사용한다.

Visual Map은 raw graph viewer도, 이론적인 UML 뷰어도 아니다. 엔진이 만든 관계를
기능별 실행 흐름과 변경 영향 다이어그램으로 투영하는 제품이다. 따라서 화면을
먼저 예쁘게 만드는 것보다, 아래 데이터가 정확히 생성되고 출처를 잃지 않는 것이
선행 조건이다.

공식 지원 언어·프레임워크·ORM·공통 기술·DB 범위는 별도 계약 문서인
[`visual-map-supported-stack-contract.md`](visual-map-supported-stack-contract.md)에
고정한다. Tree-sitter grammar가 존재하거나 외부 인덱서가 제공된다는 사실만으로
Visual Map의 정밀 분석 지원 범위가 넓어지는 것은 아니다.

## 코드 없이 먼저 이해하는 원칙

사용자는 기능의 동작과 영향 범위를 파악하기 위해 파일을 먼저 열 필요가 없어야
한다. Visual Map이 제공하는 첫 답은 코드 원문이 아니라 다음 의미 정보다.

- 어디서 시작하는가: API, 이벤트, 작업의 진입점과 입력값
- 어떻게 동작하는가: 검증, 처리 코드, DB, 외부 서비스, 성공·실패 결과
- 어떤 데이터를 다루는가: 읽고 쓰는 테이블과 컬럼
- 어디까지 영향이 가는가: 변경 대상에서 이어지는 호출·데이터·테스트
- 어디서 수정하는가: 관련 코드 파일과 줄 위치

코드 파일과 라인은 기본 화면의 전제 조건이 아니라 **검증용 원문 증거**다. 사용자가 필요할 때만 열 수 있어야 하며, 파일을 열지 않아도 주요 질문의 답과 판단 근거를 이해할 수 있어야 한다.

단, 코드를 숨겨 신뢰성을 만드는 것은 아니다. 화면에 그린 모든 실행·데이터
연결은 반드시 파일·라인·생성 전략으로 역추적 가능해야 한다. 엔진이 연결을
검증하지 못한 경우에는 실행 흐름에 선을 그어 사용자가 판단하게 만들지 않는다.
그 연결은 화면에서 제외하고, 원인과 대상 파일은 엔진 진단 결과로 보존한다.

### 화면 신뢰성 원칙

- 기본 다이어그램에는 검증된 실행·데이터 연결만 표시한다.
- `confirmed`, `candidate`, `unknown`, `confidence` 같은 내부 상태를 일반 화면의
  선, 색, 배지, 문구로 사용자에게 판단시키지 않는다.
- 검증하지 못한 연결을 후보 선으로 그려 실제 흐름처럼 보이게 하지 않는다.
- 분석 누락·파싱 실패·동적 코드 정보는 실행 다이어그램과 분리된 엔진 진단
  결과로 저장한다. 사용자가 추정 관계를 직접 판정해야 하는 제품으로 만들지 않는다.
- 화면은 “이 연결이 확실한가?”를 묻는 대신 “검증된 실제 흐름이 무엇인가?”에
  답해야 한다.

## 제품이 답해야 하는 질문

### 기능 동작 이해

- 기능은 어떤 API, 이벤트, 작업에서 시작하는가?
- 어떤 입력값을 받고 어디서 인증·검증하는가?
- 어떤 Handler, 함수, 서비스, 저장소 코드를 통과하는가?
- 어떤 테이블과 컬럼을 읽거나 쓰는가?
- DB가 연결되지 않아도 코드가 어떤 DB 접근을 시도하는가?
- 외부 서비스, 큐, 이벤트와 어디서 만나는가?
- 성공과 실패 결과는 어디에서 갈라지는가?
- 관련 코드는 실제 파일의 어느 위치에 있는가?

### 변경 영향

- 이 함수/메서드를 바꾸면 누가 호출하는가?
- 이 API를 바꾸면 어떤 처리 경로와 DB 객체에 닿는가?
- 이 테이블/컬럼을 바꾸면 어떤 코드가 깨질 수 있는가?
- 이 기능을 수정하면 어떤 API, 코드, DB, 테스트가 영향을 받는가?
- 다음에 개발자가 열어볼 파일과 라인은 어디인가?

## 책임 경계

```text
codebase-memory  -> 코드 사실과 코드 관계
database-memory  -> RDB 스키마 사실과 DB 관계
통합 어댑터       -> 두 엔진의 식별자 정규화와 스냅샷 조인
Visual Map        -> 질문 중심 투영, 정렬, 접기, 근거 표시
```

어댑터와 UI는 엔진이 놓친 관계를 사실처럼 만들어서는 안 된다. 엔진이 반환하지
않은 관계는 `confirmed`가 될 수 없으며, 일반 실행 다이어그램에도 그리지 않는다.

## 그래프의 기준 단위

관계만 저장하면 오래된 분석 결과와 현재 코드를 구분할 수 없다. 그래프는 다음 분석 단위를 가져야 한다.

| 노드 | 필수 정보 | 역할 |
| --- | --- | --- |
| `Project` | `project_id`, 이름, 저장소 경로 | 프로젝트 식별 |
| `Snapshot` | `snapshot_id`, branch, commit, indexed_at, engine_version | 특정 시점의 결과 |
| `AnalysisRun` | 상태, 시작/종료 시각, 오류, 처리 파일 수 | 이번 분석의 완료 여부 |
| `LanguageCapability` | 언어, 문법/해석 범위, 제한 | 지원 범위와 한계 |

모든 Visual Map 결과는 `snapshot_id`와 분석 상태를 가져야 한다. 오래된 스냅샷을 현재 코드인 것처럼 보여주면 안 된다.

분석 상태와 관계 검증 상태는 별개다. 관계 검증 상태는 엔진과 통합 어댑터가
잘못된 연결을 화면에 내보내지 않기 위한 내부 품질 관리 데이터다.

- 분석 상태: `complete`, `partial`, `failed`, `stale`, `unsupported`
- 내부 관계 검증 상태: `confirmed`, `structural`, `candidate`, `unknown`

`unknown`은 “관계가 없다”는 뜻이 아니다. 정적 분석으로 결론을 낼 수 없다는 뜻이다.

## 코드 엔진 필수 데이터

### 노드

모든 코드 노드는 최소한 다음 값을 가져야 한다. 현재 codebase-memory의 graph buffer 필드(`id`, `label`, `name`, `qualified_name`, `file_path`, `start_line`, `end_line`, `properties_json`)를 기준으로 한다.

| 필드 | 용도 | 필수 규칙 |
| --- | --- | --- |
| `id` | 관계의 끝점 | 한 스냅샷에서 유일해야 한다. |
| `label` | Route, Function, Method, Class, File 등 타입 | UI가 이름 추측으로 타입을 정하면 안 된다. |
| `name` | 짧은 표시명 | 원본 심볼명 보존. |
| `qualified_name` | 조인·탐색의 정규 식별자 | 프로젝트 내에서 결정적이어야 한다. |
| `file_path` | 파일 열기 | 프로젝트 루트 기준 상대 경로. |
| `start_line`, `end_line` | 근거 위치 | 1-based 라인. 없으면 거짓 위치를 만들지 않는다. |
| `properties_json` | 언어별 시그니처·HTTP·프레임워크 메타데이터 | JSON 파싱 실패 시 노드를 완전한 사실로 취급하지 않는다. |

### 반드시 보존해야 하는 코드 노드 종류

1. `File`, `Module` 또는 동등한 파일/모듈 경계
2. 외부 진입점인 `Route`/`API`
3. `Function`, `Method`, 생성자
4. `Class`, `Interface`, `Trait`, `Struct`, `Enum` 등 타입
5. 프레임워크가 식별한 Handler/Controller/Resolver
6. 외부 서비스·메시지·비동기 경계가 코드에서 직접 식별되는 경우의 서비스 노드
7. 테스트와 설정 노드처럼 영향도 판단에 필요한 참조 대상

언어별 AST 이름이 다르더라도 Visual Map에 들어올 때 위 의미 중 하나로 정규화한다. 단순히 모든 AST 노드를 그래프로 넣지 않는다.

### 공통 진입점 모델

HTTP API만 특별 취급하지 않고 모든 실행 시작점을 `EntryPoint` 의미로 정규화한다.

```text
HTTP_ROUTE       POST /orders/{id}
RPC_ENDPOINT
EVENT_CONSUMER  OrderCreated
QUEUE_HANDLER
SCHEDULED_JOB
CLI_COMMAND
WEBHOOK
```

진입점에는 `kind`, method/path 또는 event_name, handler_symbol_id, framework, source_file, source_line이 필요하다. 이를 통해 API·이벤트·배치를 같은 실행 경로 화면에서 읽을 수 있다.

## 관계 계약

### 확정 관계: 화면의 기본 경로에 사용

다음 관계는 소스 문법, AST/LSP, 명시적 프레임워크 규칙 등 재현 가능한 근거가 있을 때만 생성한다.

| 관계 | 방향 | Visual Map 의미 | 최소 근거 |
| --- | --- | --- | --- |
| `DEFINES` | File/Module -> symbol | 파일이 심볼을 정의함 | 정의 노드 위치 |
| `CONTAINS_FILE` | Project/Package -> File | 프로젝트 구조 | 실제 파일 경로 |
| `IMPORTS` | File/Module -> imported symbol/module | import/use/require | import 구문 위치와 원본/별칭 |
| `CALLS` | caller -> callee | 실제 실행 호출 경로 | 호출 AST/LSP 근거, caller/callee 위치 |
| `HANDLES` | Route/API -> Handler | API 진입점의 실제 처리 대상 | route 등록 구문과 handler 식별 |
| `INHERITS` | subtype -> base | 상속을 통한 도달 가능성 | 선언부의 base type |
| `IMPLEMENTS` | type -> interface/trait | 계약 구현 관계 | 선언부의 interface/trait |
| `DECORATES` | decorator -> decorated symbol | 프레임워크/런타임 연결 | decorator 위치 |
| `TESTS`/`TESTS_FILE` | test -> target | 변경 후 확인할 테스트 | 테스트 참조/파일 근거 |
| `HTTP_CALLS` | code -> external route/service | 외부 HTTP 경계 | 라이브러리 호출 + URL/endpoint 인자 |
| `ASYNC_CALLS` | code -> broker/topic/job | 비동기 경계 | publish/consume/queue 구문 근거 |

프로젝트 구조와 타입 탐색을 위해 다음 관계도 보존한다.

| 관계 | 의미 |
| --- | --- |
| `EXPORTS` / `REEXPORTS` | 모듈이 외부에 노출하거나 다시 노출하는 심볼 |
| `AWAIT_CALLS` | 비동기 호출 경계 |
| `CONSTRUCTOR_CALLS` | 생성자 호출과 생성 타입 |
| `DISPATCHES_TO` | 타입/런타임 dispatch 결과 |
| `OVERRIDES` | 상위 타입 메서드 재정의 |
| `TRAIT_USES` | trait/mixin 사용 |
| `RETURNS_TYPE` / `ACCEPTS_TYPE` / `USES_TYPE` | 타입 기반 호출 해석 근거 |

`CALLS`는 이름이 같다는 이유만으로 만들지 않는다. 같은 파일의 직접 호출, import가 해석된 호출, 타입이 확정된 메서드 호출을 우선하고, 해석되지 않은 호출은 별도 상태로 남긴다.

### 코드-DB 관계: 통합 스냅샷에서만 확정

코드 엔진이 수집해야 하는 것은 “DB 이름 후보”가 아니라 DB 접근의 의미를 판정할 원본 근거다.

| 관계 | 방향 | 의미 | 확정 조건 |
| --- | --- | --- | --- |
| `EXECUTES_QUERY` | Function/Method -> query/site | 코드가 SQL/ORM 쿼리를 실행 | 정적 SQL 또는 ORM 호출이 AST에서 식별됨 |
| `READS` | code/query -> table | 테이블 조회 | 쿼리의 FROM/SELECT 또는 ORM read 근거 |
| `WRITES` | code/query -> table | 테이블 변경 | INSERT/UPDATE/DELETE/UPSERT 또는 ORM write 근거 |
| `USES_COLUMN` | code/query -> column | 특정 컬럼 사용 | SQL 식별자/ORM 필드가 유일하게 해석됨 |
| `MAPS_TO` | model/entity -> table | 모델과 테이블 매핑 | 명시적 ORM annotation/config 근거 |

테이블과 컬럼의 실제 존재, PK/FK/index, 제약 이름은 `database-memory`가 책임진다. 코드 엔진은 DB 엔진이 가진 스키마를 복제하지 않고, 코드 쪽 접근 근거와 정규화된 이름을 내놓는다.

### DB 미연결 상태의 코드 측 DB 정보

DB 연결은 선택 사항이지만, DB 연결 여부가 코드 분석 결과를 줄여서는 안 된다. 코드
엔진은 DB snapshot 없이도 코드 안에서 관찰되는 DB 접근 정보를 모두 보존해야 한다.

코드 엔진이 제공해야 하는 정보:

- 사용한 DB driver/client와 ORM
- DB 접근을 수행하는 함수·메서드·Repository
- 읽기·쓰기·삭제·upsert·DDL·transaction 동작
- 정적으로 확인되는 SQL 원문 또는 정규화된 query fingerprint
- SQL의 table/column 이름, alias, CTE, join 관찰값
- ORM model/entity와 코드에 선언된 table/column mapping
- query parameter와 코드 값의 연결을 알 수 있는 범위
- 반환 row를 model/DTO로 변환하는 코드
- migration이 생성·변경·삭제하는 DB 객체
- connection/config/environment 접근 위치(비밀값 자체는 저장하지 않음)

DB가 연결되지 않은 경우의 코드 측 모델은 다음과 같다.

```text
Function/Repository
  -> EXECUTES_QUERY
  -> Query(operation=read)
  -> REFERENCES_TABLE_NAME("users")
  -> USES_COLUMN_NAME("email")
```

이 결과는 “실제 DB에 `users` 테이블이 존재한다”는 뜻이 아니다. 그것은 코드에서
확인한 query 사실이며, UI에서는 코드 흐름의 DB 접근 단계로 표시한다. DB snapshot이
연결되면 통합 어댑터가 이 이름을 실제 `database/schema/table/column` stable key와
대조하고, 일치할 때만 `READS`, `WRITES`, `USES_COLUMN`, `MAPS_TO` 통합 관계를
생성한다.

따라서 다음 두 결과를 섞지 않는다.

```text
코드만 연결:
  UserRepository -> Query -> users.email 참조

코드 + DB 연결:
  UserRepository -> Query -> public.users.email 실제 객체
```

DB 미연결은 코드 DB 접근 정보를 숨기는 이유가 아니며, DB 엔진이 없는데 실제
스키마 관계를 만들어내는 이유도 아니다.

### Query와 DataAccess 중간 노드

함수에서 테이블로 바로 선을 긋지 않는다. SQL/ORM 접근을 설명할 수 있도록 필요하면 다음 중간 노드를 만든다.

- `Query`
- `DataAccess`
- `ORMModel`
- `RepositoryOperation`

`Query`에는 operation(read/write), source 위치, SQL fingerprint 또는 raw SQL hash, parser, ORM framework를 보존한다. 대표 경로는 다음과 같다.

```text
RepositoryOperation
  -> EXECUTES_QUERY
  -> READS/WRITES
  -> Table
  -> USES_COLUMN
  -> Column
```

이 중간 노드가 있어야 “조회인지 수정인지”, “어떤 컬럼을 쓰는지”, “SQL/ORM 근거가 무엇인지”를 코드 원문 없이 설명할 수 있다.

DB 엔진 쪽에는 필요에 따라 `Database`, `Schema`, `Table`, `Column`, `Index`, `PrimaryKey`, `ForeignKey`, `View`, `StoredProcedure`, `Migration`을 보존하고, `REFERENCES`, `HAS_INDEX`, `CREATED_BY_MIGRATION`을 통해 변경 영향의 끝점을 제공한다.

## 모든 관계에 필요한 근거 메타데이터

관계의 `properties_json` 또는 통합 스냅샷의 정규 필드에는 다음 정보가 있어야 한다.

| 필드 | 설명 |
| --- | --- |
| `evidence` | 관계를 만든 근거의 요약. 예: `from orders import OrderService`, `SELECT ... FROM orders` |
| `strategy` | `ast_direct`, `lsp_method`, `import_map`, `route_registration`, `orm_mapping` 등 재현 가능한 방식 |
| `confidence` | 0~1 또는 0~100의 단일 규격. 문자열 점수 금지. |
| `source_file`/`line` | 관계를 확인할 원본 위치 |
| `target_file`/`target_line` | 대상 정의 위치를 알 수 있을 때 보존 |
| `engine_edge_type` | 원본 엔진 관계 타입. 투영 과정에서 잃지 않는다. |
| `truth_class` | `confirmed`, `structural`, `candidate`, `unknown` 중 하나 |
| `analysis_scope` | 전체/부분 인덱스, 제외 파일 수, 제한 이유 |

### 관계 검증 단계 규칙

- `confirmed`: 직접적인 소스/AST/LSP/DB catalog 근거가 있고 대상이 유일하게 식별됨.
- `structural`: import, 상속, 파일 포함처럼 구조는 확정되지만 실행 도달성은 의미하지 않음.
- `candidate`: 이름·경로·관례 기반으로 검토 가치가 있지만 실행 관계로 단정할 수 없음.
- `unknown`: 분석 범위 밖, 동적 코드, 파싱 실패, 외부 의존성 등으로 결론을 낼 수 없음.

`candidate`와 `unknown`은 엔진 진단을 위한 상태다. 일반 실행 다이어그램에는
`confirmed` 관계만 사용한다. `candidate`와 `unknown`을 색상, 점선, 배지로 그려
사용자에게 직접 판정을 맡기지 않는다. 화면에서 제외된 원인과 대상은 별도의
엔진 진단 결과로 남겨야 한다.

## Visual Map의 고정 읽기 모델

### 기본 화면: 기능을 선택하는 시작점

기본 출력은 전체 노드를 한꺼번에 그리는 그래프가 아니라, 사용자가 기능/API를
선택해 실제 흐름을 펼치는 시작점이다.

- 도메인/모듈/패키지 그룹
- 각 그룹의 기능/API 목록
- 기능별 진입점과 대표 실행 흐름
- 기능을 선택했을 때 펼칠 수 있는 코드·DB·외부 서비스 범위

개요의 연결선은 검증된 `HANDLES`, `CALLS`, `HTTP_CALLS`, `ASYNC_CALLS`만
표시한다. 후보 연결을 기본 화면에 넣지 않는다.

### 기능/API: 실제 실행 흐름을 읽는 화면

기능/API를 선택하면 다음 순서의 다이어그램을 제공한다.

```text
EntryPoint/API
  -> Input
  -> Auth/Validation
  -> Handler/Service/Function
  -> Repository/Query
  -> Table/Column (READS/WRITES)
  -> ExternalService/Event
  -> Success/Error result
```

각 단계는 실제 이름, 역할, 입력·출력, 읽기·쓰기 동작을 보여준다. 사용자가
노드를 선택하면 해당 코드 파일과 줄 위치를 연다. 실행 연결을 검증하지 못한
단계는 가짜 카드나 후보 선으로 채우지 않는다.

로그인 기능의 대표 화면은 다음 정보를 포함해야 한다.

```text
POST /auth/login
  input: email, password
    -> LoginHandler
    -> credential validation
    -> users.email / users.password_hash READ
    -> TokenService
    -> refresh_tokens INSERT
    -> login_histories INSERT
    -> token response
```

실패 경로도 같은 읽기 모델에서 표현한다.

```text
credential validation
  -> success -> token response
  -> failure -> 401 response
```

### 데이터베이스: 테이블 사용 위치

테이블을 고르면 다음 순서로 보여준다.

1. 테이블의 확정 PK/FK/컬럼 구조는 database-memory 결과
2. 이 테이블을 `READS`하는 코드
3. 이 테이블을 `WRITES`하는 코드
4. 컬럼별 `USES_COLUMN`과 근거
5. 이 테이블을 읽거나 쓰는 검증된 기능 흐름

### 변경 영향: 선택 대상을 중심으로 역방향 탐색

함수/API/테이블/컬럼 하나를 기준으로 다음을 분리한다.

- 직접 영향: 한 단계의 확정 incoming/outgoing 관계
- 간접 영향: 확정 관계를 따라 도달한 경로
- 관련 테스트와 검증된 실행 경로

영향 범위의 상한이나 접기 때문에 숨긴 항목은 `truncated=true`와 숨긴 개수를
내부 결과에 보존한다. 화면은 검증되지 않은 관계를 영향선으로 그리지 않는다.

### 복수 대상

복수 선택은 임의의 거대한 그래프가 아니라 관계 분석으로 제한한다.

- 공통 경로: 선택한 대상 모두에 연결된 관계
- 개별 경로: 특정 대상에만 존재하는 관계
- 관계 행렬: API×Table, Function×API처럼 의미가 명확한 조합
- 연결되지 않은 대상: 선택은 됐지만 검증된 관계가 없는 대상

예를 들어 API 3개와 테이블 3개를 고르면 각 셀을 검증된 `READ`, `WRITE`, `-`로
표시할 수 있어야 한다. 연결되지 않은 대상도 결과에 포함하되, 추정 연결을
추가하지 않는다.

### 엔진 진단 목록

프로젝트 이해와 유지보수를 위해 다음 상태를 별도 목록으로 제공한다.

- API는 있지만 확정 Handler가 없음
- Handler 이후 CALLS가 끊김
- 테이블은 있지만 코드 사용처가 없음
- API/이벤트에서 도달하지 않는 코드 영역
- 검증되지 않아 실행 다이어그램에서 제외된 영역
- 파싱 실패·동적 dispatch·동적 SQL 영역

이 목록은 사용자가 다이어그램의 연결을 직접 판정하기 위한 화면이 아니다.
엔진과 개발자가 누락 원인을 찾아 분석 정확도를 높이기 위한 별도 진단 결과다.

## 입력이 없는 경우의 규칙

- DB를 연결하지 않은 프로젝트: 코드 화면은 정상 제공하고 DB 영역은 `미연결` 상태로 표시한다.
- DB를 연결하지 않은 프로젝트: 코드 안의 Query/DataAccess/DB reference와 읽기·쓰기·
  테이블·컬럼 이름 관찰값은 제공하고, 실제 DB 객체와의 통합 관계만 만들지 않는다.
- 코드 인덱스가 끝나지 않은 프로젝트: 테스트 데이터나 fallback 숫자를 표시하지 않고 로딩/진행/실패 상태를 표시한다.
- 파일 파싱 실패: 해당 파일을 성공한 것처럼 포함하지 말고 파일·오류·재시도 방법을 표시한다.
- 관계가 없음: `0개`와 “관계 없음”을 구분한다. 관계를 찾지 못한 것과 엔진이 실행되지 않은 것은 같은 상태가 아니다.
- 외부 라이브러리: 소스가 인덱싱되지 않았다면 external boundary로 표시하고 프로젝트 내부 코드로 연결하지 않는다.

### 정적 분석 한계의 명시

- wildcard import는 실제 binding이 유일하게 확인될 때만 확정하며, 이름 하나만 일치하면 후보로 둔다.
- re-export와 `__all__`은 `REEXPORTS`를 따라 최종 심볼까지 연결하지 못하면 확정 호출을 만들지 않는다.
- module alias는 alias -> module -> exported symbol 순서로 해석한다. module alias 자체만 발견한 경우 후보로 둔다.
- `importlib`, 문자열·환경변수 기반 dynamic import는 실행하지 않고 확정 `CALLS`를 만들지 않는다.
- DI container, reflection, 동적 ORM 조건은 정적 근거가 없으면 확인 불가로 표시한다.

## 코어 엔진 완료 기준

다음 조건을 만족해야 Visual Map의 코드 코어를 “완료”로 부른다.

1. 동일한 입력에서 sequential/parallel 결과의 노드·관계·근거가 동일하다.
2. full index와 incremental re-index에서 삭제·이동된 파일의 고아 노드/엣지가 남지 않는다.
3. 지원 언어마다 최소한 File, 정의, import, 직접 CALLS, 상속/구현, 위치를 검증하는 fixture가 있다.
4. 프레임워크별 Route/Handler와 HTTP/async 경계를 별도 fixture로 검증한다.
5. 정적 SQL/ORM 접근은 `EXECUTES_QUERY`와 `READS`/`WRITES`/`USES_COLUMN`의 근거를 보존한다.
6. 의도적으로 틀린 이름·동명이인·동적 호출 fixture에서 잘못된 `confirmed` 관계를 만들지 않는다.
7. 분석 범위 밖의 결과는 숨기지 않고 `candidate` 또는 `unknown`과 원인을 함께 반환한다.
8. Visual Map은 위 계약에 없는 노드·관계·숫자를 만들어 화면을 채우지 않는다.
9. 후보·미확인·지원하지 않는 문법은 관계 없음과 구분되고 원인이 보인다.
10. re-export, alias, wildcard, dynamic import 변형 테스트에서 잘못된 확정 관계를 만들지 않는다.

## 검증 시나리오

| 시나리오 | 반드시 확인할 결과 |
| --- | --- |
| 처음 보는 프로젝트 | 개요에서 대표 Route를 선택해 1분 안에 Handler와 주요 모듈을 찾음 |
| API 수정 | Route -> Handler -> CALLS -> DB 접근 경로와 각 파일/라인을 확인함 |
| 함수 수정 | 직접/간접 호출자와 테스트를 분리해 확인함 |
| 테이블 변경 | READ/WRITE 코드, 컬럼 사용, FK 영향과 관련 테스트를 확인함 |
| DB 미연결 | 코드 관계는 유지되고 DB를 임의로 추정하지 않음 |
| 동적 코드 | 실행 다이어그램에 가짜 연결을 만들지 않고 엔진 진단에 기록함 |
| 여러 대상 선택 | 선택 대상 사이의 실제 관계만 같은 읽기 문법으로 투영함 |

## 사용성 완료 기준

다음 질문은 사용자가 코드 파일을 열지 않고도 첫 답을 얻을 수 있어야 한다.

- 이 프로젝트의 주요 API와 처리 영역은 무엇인가?
- 선택한 API는 어떤 Handler와 함수로 이어지는가?
- 선택한 함수/API/테이블을 바꾸면 어디까지 영향이 가는가?
- 어떤 테이블과 컬럼을 읽거나 쓰는가?
- 선택한 기능의 실제 실행·데이터 흐름은 무엇인가?

파일·라인 열기는 다음 경우의 보조 동작이다.

- 확정 관계의 원문을 감사할 때
- 엔진 진단에서 누락 원인을 확인하거나 잘못된 연결을 수정할 때
- 동적 dispatch, 동적 SQL, 외부 라이브러리처럼 자동 결론이 불가능할 때

이 문서의 필드와 상태가 바뀌면 엔진 테스트, Rust 통합 모델, TypeScript 화면 테스트를 함께 수정한다. 문서와 구현이 다르면 화면의 현재 동작이 아니라 이 계약을 기준으로 결함을 기록한다.
