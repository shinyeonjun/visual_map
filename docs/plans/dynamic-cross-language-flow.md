# 다언어 동적 실행 흐름 개선 계획

- 상태: 설계 승인 대기
- 연구 문서: `docs/research/dynamic-cross-language-flow.md`
- 목표: 프로젝트마다 동적으로 탐색하되, 확인 가능한 관계만 확정 실행 흐름으로 보여준다.

## 완료 정의

다음 조건을 모두 만족해야 “다언어 실행 흐름 지원”으로 간주한다.

1. 프로젝트를 열면 실제 파일·언어·프레임워크·진입점이 하드코딩된 폴더 규칙 없이 발견된다.
2. 지원 등급과 capability가 언어·프레임워크·분석 항목별로 표시된다.
3. `EntryPoint -> Handler -> Service -> Repository/Query -> DB` 흐름의 각 관계에 source/line/provider 증거가 있다.
4. 모호성·미지원·부분 실패·stale 상태는 `gap` 또는 `unknown`으로 보이며 확정 관계로 위장하지 않는다.
5. 코드-DB 확정 관계는 database-memory의 정확한 스냅샷 매칭 뒤에만 생성된다.
6. 동일 의미의 이름·파일 이동·선언 순서 변경, 동명이인, 부분 실패를 포함한 변형 테스트가 통과한다.
7. Petclinic, node-express-boilerplate 외 최소 한 개 이상의 실프로젝트와 언어별 fixture에서 같은 계약을 검증한다.
8. 대형 저장소에서 파일·관계·hop·시간 상한을 지키고 취소 및 stale 캐시가 안전하게 동작한다.

## 단계별 작업

### Phase 0 — 계약 고정과 기준선

- 현재 `CodeInventory`, calls/handles/relationGaps, database-memory 조인 경계를 재사용한다.
- capability, 분석 실행 상태, 증거, gap의 필드와 승격 규칙을 공통 계약에 명시한다.
- 기존 Petclinic·node fixture의 기대값을 “전체 완성”이 아니라 현재 지원 등급의 기준선으로 정리한다.

검증 게이트: 기존 테스트·빌드가 유지되고, 기존 결과의 의미가 바뀌지 않는다.

### Phase 1 — 공통 capability와 실행 결과 정규화

- provider/framework 결과를 언어별 UI 분기 없이 공통 capability로 투영한다.
- `language`, `provider`, `framework`, `feature`, `supportTier`, `status`, `reason`, `evidenceCount`를 기록한다.
- snapshot/run 식별자와 현재 소스 일치 여부를 모든 화면 데이터에 연결한다.

검증 게이트: 지원·미지원·부분 성공을 각각 fixture로 만들고 잘못된 `confirmed` 승격이 없는지 확인한다.

### Phase 2 — 공통 flow traversal

- 진입점 종류를 공통화하고, bounded hop으로 직접 확인된 호출 경로를 계산한다.
- 중복 노드와 중복 관계는 안정적인 ID와 정규화 규칙으로 제거한다.
- ambiguity, unresolved, provider failure를 경로에서 삭제하지 말고 gap으로 보존한다.

검증 게이트: cross-file, 동명이인, 순환 호출, 비동기 호출, 부분 provider 실패 테스트를 통과한다.

### Phase 3 — 언어/provider 품질 매트릭스

우선순위는 실제 검증 수요와 현재 코드 기반을 기준으로 TypeScript/JavaScript, Java/Kotlin, Python, C#/.NET, Go/Rust 순으로 둔다. 이후 C/C++, Swift, PHP, Ruby, Dart를 같은 계약으로 확장한다.

각 언어는 별도 화면이 아니라 다음 fixture 묶음을 제공한다.

- 정의·참조·import/module
- cross-file 직접 호출
- overload/receiver 또는 동등한 언어 특성
- async/generator 또는 동등한 실행 경계
- 동명이인·모호성·미지원 패턴

검증 게이트: 언어별 최소 품질 지표와 실패 사유가 capability에 반영된다.

### Phase 4 — framework/ORM/DB 통합

- framework pack은 route ownership, handler, middleware, DI, consumer, scheduler를 담당한다.
- ORM/SQL은 query evidence와 DbReference를 추출하되, 정확한 database-memory 조인 전에는 DB 관계를 확정하지 않는다.
- 지원 팩이 아닌 이름 유사 framework는 generic 구조 분석으로만 남긴다.

검증 게이트: route 중복 제거, route-handler ambiguity, ORM query false positive, DB snapshot mismatch를 검증한다.

### Phase 5 — UI 동적 투영

- Project Explorer의 언어·프레임워크·지원 등급·분석 상태를 데이터에서 생성한다.
- 실행 흐름에는 확정 관계만 기본 표시하고 후보/갭은 별도 설명 영역에서 확인하게 한다.
- 모든 노드·관계 선택은 source 파일, 라인, provider, 사유로 역추적된다.
- 고정 footer/statusbar와 같은 레이아웃 경계는 긴 프로젝트 목록·다국어 라벨·빈 상태에서 검증한다.

검증 게이트: 화면이 잘리지 않고, 0건·부분 성공·대량 목록·혼합 언어 프로젝트에서 상태가 왜곡되지 않는다.

### Phase 6 — 범용성·성능·출시 검증

- 실제 공개 저장소 샘플, 생성 코드, monorepo, 혼합 언어, 대형 파일을 추가한다.
- property/metamorphic/differential/negative 테스트를 CI에 묶는다.
- 파일·노드·관계·시간·메모리 상한과 취소를 측정한다.
- 실패 시 해당 capability만 격리하고 다른 언어·프로젝트 결과를 보존한다.

출시 게이트: 지원 팩별 필수 fixture 통과, 주요 실제 저장소 smoke 통과, CI·패키지·설치 후 smoke 통과.

## 구현하지 않을 것

- 모든 신규 framework를 이름만으로 자동 지원한다고 표시하지 않는다.
- JavaScript 동적 디스패치, DI, reflection, 코드 생성을 추측해 확정하지 않는다.
- 언어마다 별도 화면과 별도 관계 모델을 만들지 않는다.
- 런타임 실행을 기본 분석으로 추가하지 않는다. 필요하면 별도 선택 기능과 보안 경계를 먼저 정의한다.
- 한두 개 공개 저장소의 출력만 맞추는 특례 규칙을 추가하지 않는다.

## 위험과 완화

| 위험 | 완화 |
| --- | --- |
| 동적 언어의 실제 대상 불명확 | 후보/unknown/gap 유지, 직접 증거가 없으면 확정 금지 |
| provider별 ID·라인 불일치 | 공통 정규화와 source evidence 계약, differential fixture |
| framework pack 오탐 | manifest/구조/문법의 복합 조건과 negative fixture |
| DB 문자열 매칭 오탐 | database-memory exact snapshot join |
| 대형 저장소 성능 저하 | bounded traversal, 상한, 취소, 단계별 캐시 |
| 현재 소스와 stale 결과 혼동 | snapshot/run ID와 stale 상태를 UI까지 전달 |

## 롤백 기준

새 capability나 traversal이 기존 확정 관계 수를 근거 없이 늘리거나, 부분 실패를 성공으로 바꾸거나, 기존 지원 fixture의 결과를 바꾸면 해당 단계만 비활성화한다. 공통 계약과 기존 provider 결과를 유지한 채 다음 단계로 진행하지 않는다.

## 다음 실행 단위

제품 정책 승인 후 Phase 0의 계약·기준선부터 시작한다. 첫 코드 변경은 공통 capability/품질 투영과 테스트이며, 언어별 UI 분기나 새로운 framework 특례가 아니다.
