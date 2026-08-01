# 지원 언어 공통 핵심 품질 도달성 분석

- 상태: 개발 기준 확정 필요
- 규모: Large
- 목표: 모든 활성 지원 언어가 같은 핵심 분석 완성도와 같은 실패 처리 품질을 갖도록 한다.

## 판정

도달 가능하다. 단, 현재 상태는 목표에 도달하지 않았다.

현재 bridge는 12개 언어를 실제 구현한다. 반면 제품 지원 계약에는 Kotlin과 Swift를 포함한 14개 언어가 적혀 있다. framework pack도 12개 언어와 84개 pack을 기준으로 한다. 따라서 먼저 “지원 언어”라는 집합을 코드·pack·문서·release gate에서 하나로 맞춰야 한다.

또한 현재 language semantic gate는 모든 활성 언어의 cross-file `CALLS`와 source range를 검사하지만, route → middleware → handler → service → repository → DB 전체 흐름의 동일한 완성도를 검증하지는 않는다. framework semantic gate도 pack이 선언한 fact를 실행하는지 검증하는 것이지, 모든 실제 프로젝트에서 내부 호출과 DB까지 완주하는 것을 증명하지는 않는다.

## 현재 상태

| 영역 | 현재 확보 | 목표와의 차이 |
| --- | --- | --- |
| 언어 provider | 12개 bridge language, SCIP/LSP 기반 | Kotlin/Swift는 현재 provider·pack·동일 gate 없음 |
| 공통 직접 호출 | cross-file `CALLS`, source range, unresolved 방지 | 언어별 call-chain coverage 수치와 공통 승격 기준 부족 |
| framework pack | 84개 선언 pack, shared adapter, fixture gate | pack fact 통과가 실제 route-to-service 완주를 보장하지 않음 |
| Tauri adapter | calls/handles를 inventory로 정규화하고 gap 보존 | provider 누락·정규화 누락·투영 누락의 원인 전달을 더 세분화해야 함 |
| API flow | 확정 HANDLES/CALLS bounded traversal, DB exact join | 내부 service/DI/ORM chain을 모든 언어에서 동일하게 인증하지 않음 |
| UI | 프로젝트·API·코드·DB를 여러 projection으로 제공 | capability와 품질 상태를 언어별로 동일하게 노출할 기준 필요 |
| 테스트 | 언어 semantic, framework pack/provider, 일부 실프로젝트 gate | 공통 의미 fixture와 전체 경로 conformance gate 부족 |

## “동일한 품질”의 정확한 정의

언어 문법을 똑같이 해석한다는 뜻이 아니다. 다음 공통 계약을 모든 활성 언어가 통과한다는 뜻이다.

1. 동일한 source coverage 상태를 보고한다.
2. 동일한 심볼·정의·import·직접 호출 최소 기능을 제공한다.
3. 확정 관계마다 동일한 endpoint·source range·strategy 근거를 보존한다.
4. 모호성·동적 dispatch·provider 실패를 동일하게 candidate/unknown/failed로 처리한다.
5. 이름 일치만으로 확정 관계를 만들지 않는다.
6. 같은 의미의 fixture를 언어별 문법으로 구현하고 같은 expected graph를 검증한다.
7. 하나의 언어만 gate를 완화해서 release하지 않는다.

framework·ORM의 고급 기능은 언어 공통 core 위의 인증 capability로 취급한다. 예를 들어 Express middleware와 Spring DI는 문법은 다르지만, 각각 `MIDDLEWARE`, `HANDLES`, `DEPENDENCY`, `CALLS`의 같은 증거 규칙으로 평가한다.

## 필요한 구조

```text
언어 provider
  -> 공통 semantic IR
  -> framework/ORM pack
  -> 통합 linker
  -> snapshot + diagnostics
  -> Project / API / Code / DB projection
```

각 언어 provider가 별도 UI나 별도 truth rule을 가지면 품질이 다시 어긋난다. 반대로 공통 adapter만 두고 언어 의미를 무시하면 호출·DI·ORM 오탐이 생긴다. 정답은 공통 IR·공통 승격 규칙·공통 오류 모델과 언어별 의미 adapter의 조합이다.

## 최소 공통 분석 범위

모든 활성 언어가 같은 순서로 통과해야 한다.

```text
파일 coverage
  -> File/Module/Symbol
  -> import/reference
  -> cross-file direct CALLS
  -> framework EntryPoint/HANDLES (해당 pack이 있을 때)
  -> 내부 CALLS bounded flow
  -> Query/DbReference
  -> DB exact snapshot join
```

해당 언어에 HTTP framework가 없으면 HTTP route를 억지로 만들지 않는다. 대신 `unsupported/not applicable`을 명시하고, 그 언어가 지원하는 EntryPoint 종류(예: RPC, event, CLI)를 같은 계약으로 검증한다.

## 핵심 위험

### 1. 문서와 실제 지원 범위 불일치

14개를 제품 지원이라고 말하면서 실제 bridge와 pack은 12개면 사용자는 지원 품질을 잘못 기대한다. Kotlin/Swift를 추가 구현하거나, 추가 전까지는 current active 12와 target 14를 분리해 표시해야 한다.

### 2. pack fixture가 너무 얕음

현재 pack gate는 declared fact와 source marker/handler 관계를 검증한다. 이것은 pack loader가 동작한다는 증거이지, controller 내부의 service call과 DB까지 정확히 이어진다는 증거가 아니다. 모든 활성 pack에는 positive, negative, ambiguity, partial-failure, end-to-end flow fixture가 필요하다.

### 3. 실패 이유가 단계별로 소실됨

provider JSON의 unresolved 관계가 Tauri inventory gap, snapshot link, API answer, UI evidence까지 같은 `gap_id`로 이어져야 한다. 그렇지 않으면 최종 화면에서 단순히 “DB 0”으로 보인다.

### 4. recall 경쟁으로 오탐 증가

노드를 많이 그리는 것이 품질이 아니다. 확정 edge precision과 근거 완전성을 먼저 release gate로 둔다. 확정할 수 없는 관계는 결과가 비어 보여도 unknown으로 남기는 것이 맞다.

## 도달 가능성

### 가능하다고 판단하는 근거

- language bridge가 이미 공통 `CALLS`, `REFERENCES`, `IMPORTS` 출력 계약을 가진다.
- framework pack이 language와 adapter family를 명시하고 fixture gate를 갖는다.
- Tauri가 calls/handles/gaps를 정규화하는 단일 지점을 가진다.
- API flow가 확정 edge만 bounded traversal하는 fail-closed 구조다.
- DB는 code-side reference와 exact snapshot join을 분리하는 계약이 있다.

### 아직 불가능한 주장

- 14개 언어 모두에서 현재 동일한 route/handler/call-chain 품질을 이미 보장한다.
- 84개 framework pack 각각이 실제 버전별 DSL을 완전히 해석한다.
- 동적 dispatch, reflection, generated code, runtime DI를 정적 분석만으로 항상 확정한다.

## 최종 목표에 도달하는 전략

1. active language 집합을 코드·pack·문서·CI에서 일치시킨다.
2. 공통 핵심 품질 gate를 모든 active language에 동일하게 적용한다.
3. language semantic fixture를 route-to-DB 의미 fixture로 확장한다.
4. framework pack마다 route/middleware/handler/service/DI fixture를 추가한다.
5. query/ORM/DB linking을 언어별이 아니라 공통 DB proof contract로 평가한다.
6. 동일한 quality matrix를 Project 화면과 release gate에 노출한다.
7. 하나라도 baseline을 못 통과하면 해당 언어를 supported가 아닌 partial/unsupported로 표시한다.

결론적으로 목표는 장기적으로 달성 가능하지만, “모든 언어를 한 번에 같은 깊이로 개발”하는 방식은 실패한다. 공통 baseline을 먼저 모든 언어에 적용하고, 그 위에 framework/ORM capability를 같은 conformance 기준으로 올리는 방식이어야 한다.
