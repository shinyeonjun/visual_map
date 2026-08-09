# Static Pipeline Data Processing Research — 2026-08-08

> 실제 10언어 cold/warm 측정과 최적화 전후 canonical ID 대조는
> [COLD-FIRST-MULTILANGUAGE-AUDIT-2026-08-09.md](COLD-FIRST-MULTILANGUAGE-AUDIT-2026-08-09.md)에 계속 누적한다.

이 문서는 최초 코드 분석을 **정확도 저하 없이** 더 빠르게 만들기 위해, Sourcegraph SCIP,
Kythe, Glean, Salsa/Skyframe, Tree-sitter, Differential Dataflow의 처리 방식을 현재 Code Memory
파이프라인과 비교한 기술 결정 기록이다.

## 결론부터

현재 가장 큰 cold 병목은 그래프 저장, SQLite, 화면 렌더링이 아니다. Python provider가 884개 파일에서
15,041개의 LSP 요청을 만들고, 그중 `definition` 11,396개를 개별 질의하는 데 41,973ms를 쓰는 방식이다.

따라서 다음 최적화 우선순위는 다음과 같다.

1. **컴파일러/언어 분석기를 한 번 실행해 전체 의미 index를 받는 batch semantic dump**를 Python에서
   그림자 실행으로 검증한다.
2. 파일·설정·toolchain digest로 analysis unit을 content-addressed하게 만들고, 변경되지 않은 unit의
   Language IR과 canonical fact를 재사용한다.
3. canonical fact에 생성 unit ownership을 기록해 바뀐 unit의 fact만 교체하고, 의존 unit만 역방향으로
   무효화한다.
4. 장기적으로 source→syntax→provider IR→canonical fact→semantic area를 순수 query DAG로 관리한다.
5. 편집 중 갱신에는 persistent parser와 Tree-sitter old-tree 재사용을 적용한다.

쉽게 말하면 현재 Python은 “재료 하나가 어디서 왔는지 창고에 11,396번 물어보는 방식”이다. 다음 후보는
“창고가 이미 한 번 정리한 전체 재고표를 파일 하나로 내보내는 방식”이다. 똑같은 의미 분석 결과를 받되
IPC 왕복과 반복 evaluator 호출을 없애는 것이 목적이다.

## 1. 무엇을 빠르게 하는지 먼저 분리한다

`빠르다`를 한 숫자로 합치면 잘못된 알고리즘을 고르게 된다.

| 구간 | 현재 실제 기준선 | 필요한 처리 기법 |
| --- | ---: | --- |
| 최초 정적 분석(cold) | 86,018ms | batch semantic index, pipeline overlap |
| 동일 입력 재분석(warm) | 9,334ms | content-addressed cache, immutable artifact reuse |
| 파일 일부 변경 | 아직 제품 기준선 없음 | unit ownership, reverse invalidation, query DAG |
| 지도 클릭·질의 | 별도 UI 경로 | indexed SQLite query, resident immutable snapshot |
| AI 의미 분석 | 외부 모델 E2E 미측정 | fact digest cache, bounded partition, reduce verifier |

이번 조사에서 가장 중요한 구분은 이것이다.

- SCIP batch index는 **최초 분석**도 줄일 수 있다.
- content-addressed unit, Glean ownership, Salsa/Skyframe는 주로 **두 번째 분석과 일부 변경**을 줄인다.
- Tree-sitter incremental parse는 **프로세스가 살아 있는 편집 세션**에서 효과가 크다.
- graph DB, columnar format, GPU는 현재 41,973ms의 Python semantic evaluator 병목을 직접 줄이지 못한다.

## 2. 조사한 시스템과 가져올 원리

### 2.1 Sourcegraph SCIP — semantic analyzer를 한 번 돌리고 occurrence를 일괄 배출

SCIP indexer는 compiler pipeline을 semantic analysis 완료 시점까지 실행한 뒤 AST를 순회하며 definition,
reference와 symbol role을 occurrence로 기록한다. Code Memory가 원하는 “근거가 있는 symbol과 관계”의
모양과 가장 가깝다.

현재도 TypeScript/JavaScript는 `scip-typescript`, C#은 `scip-dotnet`을 사용한다. generic SCIP reader는
이미 `python` protocol을 인식하고, exact occurrence와 Tree-sitter call-site range가 일치할 때만
`CALLS`/`CONSTRUCTS`를 만든다. 따라서 Python도 provider 결과를 같은 Language IR/canonical 계약으로
보내는 구조적 기반은 이미 있다.

가져올 원리:

- 동일 semantic program을 한 번 만들고 전체 occurrence를 배출한다.
- provider 결과는 deterministic order로 정규화한다.
- exact source range와 symbol identity가 없는 관계는 만들지 않는다.

### 2.2 Kythe — compilation unit과 content-addressed input

Kythe는 파일만 보는 대신 compiler argument, dependency, build setting을 포함한 compilation unit을
분석 입력으로 삼는다. source와 compilation record는 SHA-256으로 주소화할 수 있다.

Code Memory의 Source Census와 `AnalysisPlan`은 이미 이 방향의 절반을 구현했다. 다음 단계는 file digest만
아니라 provider version, compiler config, project manifest, relevant environment를 unit input digest에
포함하는 것이다.

가져올 원리:

- 같은 source라도 config/toolchain이 다르면 다른 unit이다.
- unit input digest가 같으면 provider IR을 다시 만들지 않는다.
- 불완전하면 gap으로 남기고, 틀린 관계를 만드는 것보다 비워 두는 쪽을 택한다.

### 2.3 Glean — immutable typed fact, ownership, reverse dependency invalidation

Glean은 fact를 typed·immutable·deduplicated하게 저장하고, 언어별 fact에서 공통 derived fact를 만든다.
증분 갱신은 각 fact가 어느 unit에서 만들어졌는지 ownership을 기록하고, unit 변경 시 그 fact와 의존 fact를
역방향으로 무효화하는 방식이다. 권장 unit 크기도 함수 하나가 아니라 파일 또는 모듈 수준이다.

가져올 원리:

- canonical fact마다 `owner_unit_id` 또는 동등한 별도 ownership relation을 가진다.
- 바뀐 unit이 소유한 fact를 새 fact set으로 원자 교체한다.
- 삭제된 fact를 참조하는 derived fact가 남지 않게 reverse dependency를 전파한다.
- Language IR은 언어별이어도 UI가 소비하는 canonical fact는 하나의 공통 schema로 유지한다.

### 2.4 Salsa와 Bazel Skyframe — 순수 query DAG와 memoization

Salsa는 입력 key에서 결과 value를 만드는 query를 memoize하고, 입력이 달라진 query와 그 하위 의존
query만 다시 계산한다. Skyframe도 immutable node와 dependency DAG, 역방향 무효화로 같은 문제를 푼다.

가져올 원리:

```text
source bytes/config/toolchain
  -> syntax facts
  -> provider Language IR
  -> canonical facts
  -> representative TracePath
  -> AI semantic areas
  -> map projection
```

각 화살표를 digest-keyed query로 만들면 “파일 하나 변경 때문에 AI 지도 전체 재생성”을 피할 수 있다.
하지만 현재 cold 42초를 먼저 줄이는 기법은 아니므로 Python batch index 검증 뒤에 진행한다.

### 2.5 Tree-sitter incremental parsing — 편집 중 old tree 재사용

Tree-sitter는 변경 범위를 old tree에 적용하고 영향을 받지 않은 subtree를 재사용할 수 있다. 현재처럼 분석
명령마다 프로세스와 parse tree를 버리면 이 장점이 사라진다.

가져올 원리:

- 앱에 persistent analysis service를 둘 때 workspace별 parse tree를 bounded cache로 보관한다.
- file watcher의 byte edit를 tree에 적용하고 changed ranges만 다시 fact로 만든다.
- cold CLI 파이프라인 최적화와 섞지 않고, edit-latency 단계에서 별도로 측정한다.

### 2.6 Differential Dataflow / semi-naive evaluation — 파생 관계가 커질 때만

입력 fact 변경분에 대해 derived relation의 delta만 계산하는 방식은 영향 경로, transitive containment,
boundary aggregation이 매우 커질 때 유효하다. 현재는 extraction이 병목이고 정적 TracePath도 bounded
query이므로 지금 도입하면 복잡도만 늘어난다.

채택 조건:

- derived fact 갱신이 전체 분석 시간의 지배 구간이 되고,
- 실제 대형 저장소에서 변경 delta 대비 full recompute 비용이 측정되며,
- ownership/query DAG만으로 목표 latency를 못 맞출 때.

## 3. 후보별 냉정한 비교

| 후보 | 최초 분석 | 동일 입력/일부 변경 | 정확도 위험 | 지금 판정 |
| --- | --- | --- | --- | --- |
| compiler/SCIP batch semantic dump | **매우 큼** | 큼 | provider parity 검증 필요 | **1순위 실험** |
| content-addressed analysis unit | 작음 | **매우 큼** | 낮음 | 2순위 |
| fact ownership + reverse invalidation | 거의 없음 | **매우 큼** | dangling 방지 설계 필요 | 3순위 |
| Salsa/Skyframe query DAG | 작음 | 큼 | 중간, 구조 변경 큼 | 단계적 도입 |
| Tree-sitter old-tree reuse | 없음 | 편집 시 큼 | 낮음 | persistent service 때 |
| pipeline stage overlap | 작음~중간 | 작음 | 낮음 | 영수증 뒤 보조 적용 |
| batch 16→64 확대 | 실측상 없음 | 없음 | burst/memory 증가 | **기각** |
| linker 무조건 병렬화 | 현재 지배 병목 아님 | 제한적 | 결정성/SQLite write 위험 | 보류 |
| graph DB 교체 | 없음 | query에만 가능 | migration 큼 | **현재 기각** |
| Arrow/columnar | 없음 | aggregation에만 가능 | dual storage 복잡도 | 현재 기각 |
| GPU | 거의 없음 | 거의 없음 | semantic toolchain 부적합 | **기각** |
| Bloom filter | 없음 | cache miss lookup에만 가능 | false positive 처리 필요 | 필요 시만 |

## 4. Python batch index 실험 결과와 주의점

`@sourcegraph/scip-python@0.6.6`을 Windows에서 실제 실행해 보니, 시작 직후 다음 코드가 실패했다.

```text
new RegExp(path.sep, "g")
SyntaxError: Invalid regular expression: /\ /g: \ at end of pattern
```

Windows의 `path.sep`가 단일 backslash인데 정규식 escape 없이 생성한 것이 원인이다. 실험용으로
`path.sep` 자체를 이중 backslash로 바꾸자 help는 실행됐지만, 다른 path 처리까지 오염되어 프로젝트
경로가 `\D:\...`가 되었고 indexer는 파일을 0개 읽었다. 3.172초에 끝난 결과는 64-byte 빈 SCIP이므로
**성능 결과로 사용하면 안 된다.**

추가 위험도 확인됐다.

- 현재 공개 패키지는 Python 3.10+와 Node 16+를 요구한다.
- 설치 경로에서 deprecated `inflight@1.0.6`, `glob@7.2.3` 경고가 발생했다.
- 따라서 공개 패키지를 그대로 production provider로 끼우는 결정은 하지 않는다.

이 결함이 batch semantic index라는 처리 방식 자체를 기각하는 근거는 아니다. 다만 다음 실험은 전역
path monkey patch가 아니라 **최소 source patch를 적용한 pinned fork** 또는 upstream 수정본으로 해야 한다.

## 5. 권장 목표 구조

```text
Source Census
  -> content-addressed AnalysisUnit
  -> Provider Adapter
       - SCIP batch index
       - compiler semantic extractor
       - LSP fallback
  -> unit-owned Language IR
  -> deterministic 2-pass Canonical Linker
  -> immutable Canonical Fact Bundle
  -> digest-keyed derived queries
       - TracePath
       - boundary aggregation
       - AI semantic areas
  -> map projection
```

중요한 점은 모든 언어를 같은 provider 구현에 강제로 넣는 것이 아니다. 언어마다 가장 정확한 extractor가
SCIP, compiler API, LSP 중 무엇인지 달라도 최종 `Language IR -> Canonical Fact` 계약, evidence, gap,
coverage, determinism gate는 같아야 한다.

provider adapter는 장기적으로 최소 다음 모드를 표현해야 한다.

```text
ScipBatch
CompilerExtractor
LspInteractiveFallback
```

Python은 `ScipBatch`를 그림자로 실행해 합격할 때만 승격하고, Python version 또는 Windows packaging이
지원되지 않으면 현재 `pyright-langserver`를 정확한 fallback으로 유지한다.

## 6. 정확도 그대로를 증명하는 승격 gate

새 provider가 더 빠르다는 이유만으로 교체하지 않는다. 다음을 모두 통과해야 한다.

1. Source Census가 승인한 884개 Python 파일을 전부 처리한다.
2. 닫힌 정답 corpus의 definition/import/call/type relation precision·recall이 현재보다 낮지 않다.
3. confirmed edge는 모두 exact file/range evidence를 가진다.
4. dangling endpoint와 evidence 없는 confirmed가 0이다.
5. 같은 입력 두 번의 normalized semantic fact set이 동일하다.
6. 실제 저장소에서 canonical count 차이를 단순 숫자가 아니라 relation kind와 대표 evidence별로 검토한다.
7. provider symbol 문자열이 달라질 수 있으므로 raw digest가 아니라 다음 정규화 집합도 비교한다.

```text
(source path, source range, relation kind,
 target path, target range, evidence path/range)
```

8. Windows 설치·실행, path, Python 환경 발견, package dependency/security를 통과한다.
9. 실패 시 빈 index를 성공으로 publish하지 않고 typed provider gap과 기존 LSP fallback을 사용한다.
10. 합격한 뒤에만 cold wall time 목표를 확정한다. 현재 단계에서 임의의 “30초 보장”을 쓰지 않는다.

## 7. 안전한 작업 순서

### Batch A — Python shadow batch index

- `scip-python`의 Windows path 결함을 최소 patch한 pinned 실험 runner를 만든다.
- 기존 LSP 결과를 제품 truth로 유지한 채 SCIP 결과를 별도 artifact로 저장한다.
- 정답 corpus와 `meeting-overlay-assistant`에서 normalized fact diff와 wall/memory receipt를 낸다.
- 합격하지 못하면 production 경로는 바꾸지 않는다.

### Batch B — unit cache key 강화

- source digest + relevant config + provider/toolchain version + project model을 unit digest에 포함한다.
- 같은 unit의 Language IR은 immutable artifact로 재사용한다.
- config 하나 변경 시 관련 unit만 miss가 나는지 검증한다.

### Batch C — fact ownership과 원자 교체

- canonical fact에 source analysis unit ownership을 기록한다.
- changed unit fact set을 transaction으로 교체한다.
- 삭제/변경된 endpoint를 가리키는 derived fact가 남지 않도록 reverse invalidation을 검증한다.

### Batch D — persistent incremental service

- file watcher와 bounded Tree-sitter tree cache를 추가한다.
- changed range만 syntax/provider query input으로 다시 만든다.
- cold, warm, one-file edit를 분리해 P50/P95와 peak memory를 잰다.

### Batch E — derived query DAG

- TracePath, boundary summary, AI semantic area를 fact digest와 query parameter로 cache한다.
- 영향을 받지 않은 semantic area와 AI 결과를 재사용한다.
- 같은 Fact Graph hash에서 지도 이름/멤버십이 바뀌지 않는지 검증한다.

## 8. 지금 하지 않을 것

- 그래프 DB로 갈아엎지 않는다. 추출기 반복 질의 병목을 해결하지 못한다.
- 무조건 모든 언어 provider를 한 번에 SCIP로 바꾸지 않는다.
- 더 빠르게 보이기 위해 definition, evidence, 큰 파일을 생략하지 않는다.
- LSP 결과가 없을 때 이름/경로 유사도로 confirmed edge를 만들지 않는다.
- Python empty index를 성공으로 간주하지 않는다.
- AI를 정적 extractor 실패의 사실 대체재로 사용하지 않는다. AI는 candidate/meaning을 만들고 근거가 없으면
  abstain한다.

## 9. 공식 참고 자료

- Sourcegraph, Writing a SCIP indexer: https://sourcegraph.com/docs/code-navigation/writing-an-indexer
- Sourcegraph, Precise Code Navigation: https://sourcegraph.com/docs/code-navigation
- Sourcegraph, `scip-python`: https://github.com/sourcegraph/scip-python
- SCIP indexer ecosystem: https://github.com/scip-code/scip
- Kythe overview: https://kythe.io/docs/kythe-overview.html
- Kythe compilation database: https://kythe.io/docs/kythe-compilation-database.html
- Kythe content-addressed storage: https://kythe.io/docs/kythe-storage.html
- Glean introduction: https://glean.software/docs/introduction/
- Glean incrementality: https://glean.software/docs/implementation/incrementality/
- Glean schema design: https://glean.software/docs/schema/design/
- Salsa internals: https://salsa-rs.github.io/salsa/how_salsa_works.html
- Salsa overview: https://salsa-rs.github.io/salsa/overview.html
- Bazel Skyframe: https://bazel.build/versions/8.2.0/reference/skyframe
- Tree-sitter incremental parsing: https://tree-sitter.github.io/tree-sitter/using-parsers/3-advanced-parsing.html
- Differential Dataflow: https://timelydataflow.github.io/differential-dataflow/

## 10. 열 언어 shadow/spike 완료 후 정정

위 7절의 Python 실험을 열 언어 전체로 확장해 완료했다. 결과와 공통 비교 계약은
[10-language provider shadow evaluation](./PROVIDER-SHADOW-EVALUATION-2026-08-08.md)을 정본으로 삼는다.

- provider 교체만으로 안전하게 속도를 얻은 언어는 0개다.
- TypeScript/JavaScript/C#은 이미 batch SCIP 계열을 사용 중이다. C#의 restore 생략은 실제 relation을
  누락해 제거했다.
- Python/Go/Rust/Dart 후보는 빠르거나 fact 총량이 많아도 current confirmed fact 보존 gate를 통과하지
  못했다.
- Java/C/C++ 후보는 Windows product packaging gate를 통과하지 못했다.
- 따라서 다음 공통 성능 작업은 provider 전면 교체가 아니라 unit cache, incremental invalidation,
  canonical ownership, derived-result cache다.

이번에 추가한 `compare-scip`은 provider symbol 문자열이 아니라 exact definition/evidence locator를 비교하고,
file byte digest와 normalized semantic digest를 분리한다. raw provider 비교는 production 승격의 필요조건일
뿐 충분조건이 아니며 report 자체도 `productionEligible: false`를 강제한다.
