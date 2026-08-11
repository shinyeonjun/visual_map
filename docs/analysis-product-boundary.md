# 분석 제품 경계

Status: authoritative, 2026-08-10.

이 문서는 Codebase Workspace가 **최종 화면에 제공할 정보**와 그 정보를
만드는 주체를 고정한다. 새로운 분석 항목은 이 문서에 먼저 추가되지 않는 한
제품 범위가 아니다.

> 정적 분석은 증명 가능한 사실을 만들고, DB 분석은 스키마 사실을 만들며,
> AI는 그 사실의 의미를 읽기 쉽게 묶고 이름 붙인다.

`D:\meeting-overlay-assistant`를 포함한 실제 저장소는 검증 corpus일 뿐이다.
특정 저장소의 폴더명, 프레임워크 구성, 도메인 이름을 제품 규칙으로 하드코딩하지
않는다.

## 1. 정적 코드 엔진 — 여기까지 한다

지원 언어는 `TypeScript`, `JavaScript`, `Python`, `Java`, `C#`, `C`, `C++`,
`Go`, `Rust`, `Dart` 10개로 고정한다. 언어마다 추출 방법은 달라도 아래 공용
Fact 계약과 실패 규칙은 같다.

정적 엔진이 소유하는 정보:

- 저장소, build target, package/module/file과 포함 구조
- class/interface/trait/struct/enum/type/function/method/constructor 등 코드 정의
- HTTP·GraphQL·RPC endpoint, job, event handler 등 코드에서 정확히 확인되는 진입점
- import/export, dependency, call/construct, extends/implements/override/type-use
- route에서 handler로 이어지는 framework binding
- 코드에 명시된 SQL·ORM, 외부 API, queue/event, cache, config 사용
- 근거가 이어지는 범위의 `API → handler → service → repository/query → DB reference`
- 모든 항목과 관계의 파일·줄·범위 근거
- 분석한 범위, 제외한 범위, 미지원·모호·실패 범위를 나타내는 coverage와 typed gap

정적 `TracePath`는 소스에 쓰인 호출의 방향과 어휘 순서를 보여준다. 분기, 반복,
지연 callback, `await`, interface/virtual dispatch 상태도 보존한다. 다만 이것은
**가능한 정적 실행 경로**이며 실제 런타임에서 반드시 그 순서로 실행됐다는 기록이
아니다.

정적 엔진이 하지 않는 것:

- 폴더명이나 비슷한 이름만 보고 관계 연결
- reflection, runtime DI, monkey patching 등에서 대상을 추측
- 실제 실행 횟수·실행 시간·운영 트래픽·런타임 분기 결과 주장
- 인증·결제·주문 같은 비즈니스 의미 이름 생성
- 새 API 위치, 리팩터링 방법, 영향도 결론 또는 미래 구조 추천

정확한 대상을 하나로 결정하지 못하면 선을 만들지 않는다. 확인 가능한 후보가
있으면 근거가 달린 `static_candidate`, 그렇지 않으면 typed gap으로 끝낸다.

## 2. DB 엔진 — 여기까지 한다

지원 계열은 PostgreSQL/YugabyteDB, MySQL/MariaDB, SQLite·호환 DDL,
SQL Server, Oracle이다. 실제 지원 표시는 adapter의 golden/live 인증을 통과한
제품·버전에만 붙인다.

DB 엔진이 소유하는 정보:

- database/catalog, schema, table/view/materialized view, column
- PK/FK/unique/check, index, trigger, sequence, routine, DB type와 지원 vendor 객체
- 포함 관계, FK 방향, view·routine dependency 등 catalog가 증명하는 관계
- nullability, type, default, generated/identity 등 catalog metadata
- 검사 범위, 제품·버전, adapter·권한과 complete/failed 상태

DB 엔진은 application row, sample value, query result를 절대 읽거나 저장하지
않는다. code-to-DB 관계는 별도 결정적 integration adapter가 **정규화된 코드
reference를 동일 DB snapshot의 정확히 한 객체와 일치시킬 때만** 만든다. 0개나
2개 이상과 일치하면 관계 대신 gap으로 남긴다.

## 3. AI 의미 엔진 — 여기까지 한다

AI는 검증된 Fact Graph와 제한된 source evidence만 입력으로 받는다.

AI가 소유하는 정보:

- 프로젝트 한 줄 요약과 검색용 별칭
- 기존 정적 region을 L0/L1 의미 영역으로 묶는 membership
- 영역의 짧고 구체적인 이름
- 영역의 한 줄 책임 요약
- `domain`, `shared`, `infrastructure`, `integration`, `structural` category
- 대표 Fact, 대표 TracePath, 대표 evidence 선택
- 근거가 약하거나 책임이 섞였을 때 구조 이름 유지 또는 미분류 처리

AI가 하지 않는 것:

- 파일, symbol, API, DB 객체 또는 정적 region 새로 생성
- 호출·DB·외부 연동 관계 생성 또는 endpoint 변경
- confirmed/candidate/gap 판정, 관계 방향, 실행 순서 변경
- LOC·파일·관계·coverage·gap 수 계산
- 존재하지 않는 근거 ID 인용
- 새 API 위치, 구조 승격, 리팩터링, 장애 원인 같은 권고 생성

AI 출력의 모든 member와 citation은 입력에 있던 ID여야 한다. 검증에 실패하면
AI 결과만 폐기하고 정적 snapshot은 그대로 공개한다. 충분한 의미 근거가 없으면
그럴듯한 이름을 만들지 않고 구조 이름으로 abstain한다.

## 4. 최종 화면에 제공하는 읽기 모델

분석 파이프라인의 완성 범위는 다음 다섯 개다.

| 읽기 모델 | 사용자에게 보이는 정보 | 권위 있는 생산자 |
| --- | --- | --- |
| Overview | L0 영역, 이름·한 줄 책임, 규모, 영역 간 관계, 미분류·gap | 이름·책임은 AI, 나머지는 Fact/DB 집계 |
| Drill-down | L1과 API·symbol·query·table, 내부 관계 | Fact/DB; 의미 묶음만 AI |
| Trace | API부터 확인 가능한 코드·query·DB reference까지의 방향 경로 | 정적 TracePath와 DB integration |
| Search | 파일, symbol, API, query, DB object, 의미 이름·별칭 | Fact/DB와 승인된 semantic revision |
| Evidence | source path/line/range 또는 DB catalog 근거 | 정적 코드 엔진 또는 DB 엔진 |

표시 규칙은 단순하다.

- 관계의 존재·방향·truth·근거는 브라우저가 추론하지 않는다.
- 수치와 상태는 결정적 read model이 계산하며 AI 문장에서 뽑지 않는다.
- 노드 정의 위치는 canonical `definitionEvidenceId`만 사용한다. 호출한 쪽의
  call-site를 호출 대상의 정의 위치인 것처럼 대신 표시하지 않는다.
- 영역 이름에는 `labelSource`와 필요한 경우 `fallbackReason`을 함께 제공한다.
  프론트는 category나 이름 모양으로 의미 이름 여부를 추측하지 않는다.
- 선택 상세의 분석 공백은 정적 scope/evidence 귀속으로만 계산하고,
  `totalCount`, 최대 16개 상세, `truncatedCount`로 제한해 제공한다.
- `unknown`과 `unmeasured`는 0이나 선으로 바꾸지 않는다.
- 근거 없는 필드는 빈칸·미측정·gap 중 정확한 상태로 표시한다.
- 한 읽기 모델에 필요한 생산자가 아직 없으면 가짜 데이터로 UI를 채우지 않는다.

## 5. 이번 완성 범위에서 제외한다

- 코드 수정, 생성, 자동 리팩터링
- 미래 구조 설계와 what-if simulation
- 새 API·DB 설계 위치 추천
- runtime tracing/APM/profiling과 운영 관측값
- Git branch/commit 비교와 협업 기능
- 앱 내 일반 대화 기능

앱 내 대화는 위 다섯 읽기 모델이 완성된 뒤 별도 단계에서 연결한다. 연결하더라도
게시된 snapshot을 근거로 설명하고 검색하는 소비자이며, Fact Graph를 수정하는
분석 생산자가 아니다.

## 6. 완료 판정

완료는 “많은 데이터를 만들었다”가 아니라 다음 조건으로 판정한다.

1. 10개 언어가 같은 Fact/evidence/coverage/gap 계약을 사용한다.
2. 언어별로 증명하지 못한 기능은 capability와 gap으로 정직하게 구분된다.
3. 지원 DB의 certified snapshot만 Fact Graph에 합쳐진다.
4. Overview·Drill-down·Trace·Search·Evidence가 모두 canonical ID로 왕복한다.
5. 같은 입력과 engine/config 버전은 같은 정적 digest를 만든다.
6. AI를 끄거나 AI가 실패해도 정적 snapshot·검색·근거·Trace는 보존된다.
   의미 영역으로 구성된 Overview는 없을 수 있으며 구조 fallback 지도를 별도로
   만들지 않는다.
7. AI는 ID·관계·수치·근거를 만들거나 고치지 못한다.
8. 대형 저장소에서도 read model은 제한된 SQLite query를 사용하며 전체 graph를
   클릭마다 메모리에 올리지 않는다.

이 여덟 조건 밖의 기능은 현재 엔진 완성도를 높이지 않는다. 구현 도중 새로운
아이디어가 나와도 먼저 보류 목록에 두고, 이 계약을 끝낸 뒤 제품 결정으로 다룬다.
