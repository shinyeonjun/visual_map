# Cold-first 10-language audit — 2026-08-09

이 문서는 Python에서 수행한 것과 같은 수준으로 활성 지원 언어 10개를 실제 저장소에서 검증하는 진행 기록이다.
목표는 **동일한 정적 사실을 유지하면서 최초 분석과 동일 입력 재분석을 줄이는 것**이다. 단순히 테스트가
통과하거나 파일 수가 맞는 것으로 합격시키지 않는다.

활성 언어는 TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust, Dart다. Ruby와 PHP는 현재
지원 계약에서 제외되어 이 감사의 대상이 아니다.

## 합격 규칙

각 언어는 다음을 모두 기록한다.

1. 고정된 실제 저장소 commit과 provider/compiler 버전
2. 완전히 비어 있는 제품 캐시에서의 cold 시간과 stage별 시간
3. 동일 입력 warm 시간
4. cold/warm canonical semantic digest와 bundle digest
5. node, edge, evidence, coverage, gap 수
6. 실제 소스 위치를 사람이 읽고 확인한 대표 정의·호출·타입·import 관계
7. 최적화 전후 ID 집합 차이. 최적화는 차이가 0일 때만 기본 경로가 된다.

같은 개수만으로는 합격이 아니다. ID와 payload가 달라졌다면 차이를 소스까지 추적한다. provider가 못 찾은
사실을 이름 유사도나 경로 추측으로 채우지 않는다.

## Python급 언어별 심층 검증 범위

공통 테스트를 언어 이름만 바꿔 실행하지 않는다. 아래의 **언어 고유 의미 경계**까지 실제 저장소에서
검증해야 해당 언어를 합격으로 판정한다.

| 언어 | 반드시 보존할 프로젝트 의미 | 사람이 원본과 대조할 대표 사실 | 대표 실패 모드 |
| --- | --- | --- | --- |
| TypeScript | `tsconfig` 상속, project reference, ESM/CJS, path alias | controller/service 호출, decorator route, interface 구현, re-export | 서로 다른 config의 `Program`을 합쳐 소유권 유실 |
| JavaScript | `jsconfig`/추론 config, ESM/CJS, JSDoc type | 함수·method 호출, require/import, prototype/class 상속 | config 없는 shard가 원본 tree를 변경하거나 작은 root마다 runtime 재시작 |
| Java | Gradle/Maven model, source set, JDK toolchain, generated-source 경계 | method 호출, extends/implements, annotation 기반 endpoint, import | JDTLS runtime과 Gradle toolchain 불일치, build-support 누락, session budget 고갈 |
| C# | solution/project graph, target framework, SDK, generated file 경계 | overload 호출, interface 구현, expression-bodied member 소유자 | SCIP occurrence의 빈 enclosing range, SDK/target mismatch |
| C | `compile_commands.json`, include path, macro/conditional context | 함수 호출, header 선언→정의, struct/type 사용 | compile context 없는 파일을 다른 TU 의미로 추측 |
| C++ | C 항목 + namespace/template/overload | overload 호출, class 상속, 선언→정의, template 사용 | anonymous namespace/name 훼손, 불완전 compile database |
| Go | `go.mod`/`go.work`, build tag, GOOS/GOARCH, generated 경계 | package 호출, interface 구현, constructor-like 호출 | build context가 다른 파일을 하나의 결과로 혼합 |
| Rust | Cargo workspace, feature, target, cfg, macro expansion 경계 | impl method 호출, trait 구현, module import | inactive feature 사실을 확정하거나 impl 내부 공개 method를 누락 |
| Dart | `pubspec`, package config, analysis options, experiment boundary | method 호출, implements/extends, package import | package metadata 없는 상태를 엔진 실패와 혼동 |

각 저장소는 다음 네 결과물을 함께 남긴다.

1. 빈 캐시 cold와 같은 캐시 warm의 전체/stage별 시간
2. cold/warm canonical semantic digest 및 SQLite bundle digest
3. provider receipt, capability gap, confirmed-without-evidence와 dangling endpoint 수
4. 원본 source line과 SQLite evidence를 함께 적은 사람 검증 표본

성능 패치는 위 네 결과 중 의미 digest와 사실 ID 집합이 기준선과 같을 때만 채택한다. provider/compiler가
원래 답하지 못한 동적 사실은 시간을 줄이기 위해 이름·경로로 보충하지 않는다.

## 고정 도구 버전

| 언어 | provider/runtime |
| --- | --- |
| TypeScript / JavaScript | Node 24.18.0, scip-typescript 0.4.0 |
| Python | pyright 1.1.411 |
| Java | JDTLS 1.57.0, Java 21.0.11 |
| C# | scip-dotnet 0.2.14 |
| C / C++ | clangd 22.1.8 |
| Go | gopls 0.23.0 |
| Rust | rust-analyzer 1.96.1 |
| Dart | Dart 3.12.2 |

## 실제 저장소 corpus

| 언어 | 저장소 | 고정 commit | 열거 파일 수 | 상태 |
| --- | --- | --- | ---: | --- |
| C | libuv | `a6d06ba` | 478 | cold/warm 완료 |
| C++ | fmt | `60ccad` | 142 | cold/warm 완료 |
| C# | EF Core v10.0.10 | `db55508` | 5,958 | 최종 cold/warm·원문 전수 검증 완료 |
| Dart | linter | `4b5399` | 630 | cold/warm 완료, metadata 한계 분리 검증 |
| Go | Prometheus | `e75af3` | 1,615 | cold/warm 완료 |
| Java | Spring Framework | `da4b31` | 11,395 | direct-call v6 cold/warm·원문 전수 검증 완료 |
| JavaScript | ESLint | `f878d2` | 2,358 | cold/warm 완료, batching shadow 분석 중 |
| Rust | Tokio | `ecd621` | 858 | 공개 impl 보정 cold/warm 완료 |
| TypeScript | NestJS | `c3bc75` | 2,129 | cold/warm 완료 |
| Python | meeting-overlay-assistant | local pinned tree | 884 approved Python files | cold/warm 완료, request 축소 shadow 대기 |

TypeScript compiler 저장소(81,368개 열거 파일)는 대표 정확도 corpus가 아니라 별도의 극한 규모 stress corpus로
유지한다.

## 현재 측정치

| 언어/corpus | cold | warm | provider 핵심 | canonical 결과 | 판정 |
| --- | ---: | ---: | ---: | --- | --- |
| Python / meeting-overlay-assistant | 약 86.0s | 약 9.3s | 72.9s, LSP 15,041 requests | nodes 5,298 / edges 12,058 / evidence 13,159 | 병목 확인 |
| C / libuv | 16.103s | 3.207s | cold 약 13.0s | nodes 2,213 / edges 3,414 / evidence 3,588 | cold/warm semantic·bundle 동일 |
| C++ / fmt | 9.527s | 2.030s | cold 약 7.051s | nodes 589 / edges 1,000 / evidence 1,121 | cold/warm semantic·bundle 동일 |
| JS / ESLint 최종 감사 | **364.736s** | **7.980s** | cold 310.148s, 31 batches | nodes 2,624 / edges 4,957 / evidence 6,622 | canonical 안전성·결정성 통과, raw UTF-16 column 8건 후속 필요 |
| JS / cross-root multi-config shadow | 119.668s | 미채택 | 83.194s | nodes 2,630 / edges 4,962 / evidence 6,627 | **불합격** |
| TS / NestJS 최종 감사 | **155.038s** | **15.395s / 14.775s** | cold 81.654s, 156 batches | nodes 6,907 / edges 18,207 / evidence 20,019 | 가짜 client route·0-width 정의 제거, cold/warm semantic·bundle 동일 |
| Go / Prometheus | 208.700s | 59.631s | 147.754s, gopls 83.389s / 40,566 requests | nodes 13,269 / edges 42,137 / evidence 59,336 | cold/warm semantic·bundle 동일, warm 후처리 병목 |
| Rust / Tokio 보정 전 기준선 | 204.539s | 11.153s | 182.589s, rust-analyzer 173.849s | nodes 7,195 / edges 7,363 / evidence 7,367 | 공개 impl 관계 누락 기준선 |
| Rust / Tokio 공개 impl 보정 | 194.993s | 10.966s | 182.430s, rust-analyzer 173.526s | nodes 7,231 / edges 7,436 / evidence 7,440 | cold/warm semantic·bundle 동일 |
| Dart / linter | 50.958s | 10.920s | 병렬 12 jobs, LSP 합계 40.939s | nodes 2,847 / edges 4,748 / evidence 4,959 | cold/warm semantic·bundle 동일 |
| C# / EF Core v10.0.10 최종 | **483.563s** | **168.572s** | cold scip-dotnet+변환 308.052s, warm provider 0ms | nodes 72,800 / edges 221,856 / evidence 327,568 | 5,174 active indexed + 360 MSBuild excluded; cold/warm semantic·bundle 동일 |
| Java / Spring 계획 강제판 | 472.081s | 104.253s | JDTLS 301.617s, 28,656 requests | nodes 64,600 / edges 127,005 / evidence 128,946 | 결정성 통과, 호출 대표성 불합격 |
| Java / direct-call v1 invalid shadow | 798.961s | 측정 제외 | JDTLS 479.223s, 95,797 requests | nodes 75,837 / edges 162,447 / evidence 175,383 | 호출 breadth 개선, target mismatch 157건으로 불합격 |
| Java / direct-call v2 invalid shadow | 195.029s | 측정 제외 | provider worker `capacity overflow`, Java definition 0 | nodes 33,233 / edges 71,422 / evidence 72,155 | 무제한 sentinel allocation 결함으로 전체 폐기 |
| Java / direct-call v3 invalid shadow | 572.484s | 측정 제외 | JDTLS 372.581s, 95,797 requests | nodes 80,626 / edges 170,800 / evidence 183,664 | 51,990 source token 중 target mismatch 1건으로 불합격 |
| Java / direct-call v4 aborted shadow | 측정 제외 | 측정 제외 | 긴 JDTLS label 정의-range 결함 확인 후 조기 중단 | 산출물 없음 | 이미 알려진 결함을 포함해 폐기 |
| Java / direct-call v5 invalid shadow | 590.804s | 측정 제외 | JDTLS 387.240s, 95,797 requests | nodes 81,944 / edges 176,181 / evidence 182,131 | 정의 token 111,097/111,097 일치, malformed declaration end로 정상 심볼 2,524개 제외·핵심 흐름 누락 |
| Java / Spring direct-call v6 최종 | 639.897s | 122.043s | JDTLS 396.021s, 95,797 requests | nodes 83,965 / edges 181,149 / evidence 187,417 | 호출·생성 51,982/51,982, 정의 113,406/113,406 source 일치; cold/warm digest 동일 |

확정 semantic digest:

- Python: `ef3d614699612a89fe21b96a8a0d30d7dc2e35a0e2251379c4244984b0fe5e80`
- C/libuv: `91fc2eeee65132926a7b9a056506c7cb8e3e77af25bc44bd2c02a24f371e1929`
- C++/fmt: `5c9d01101dd130cc7cbf1d0ea1d4206d3a43223800e9203a3159ecdcc4b5477d`
- JS/ESLint 최종 감사: `d3432da9d9aa5a16a6eb6e337f285bc35d1079564ef260439059290083b34731`
  (bundle `6596eb0741ace9a3479c21e46e816573c64e2ab3a0ae281c45ff2848d5bf00a5`)
- TS/NestJS 최종 감사: `23306b38d485bca8504741138a76c1b77c536f7f6846f5355b400429ff4d2c4e`
  (bundle `7b1fefa60ff01f2298b97155b3046d55eb0dd775e6d25d3b75b1cb66a44a2940`)
- Go/Prometheus: `6e45ece02bdebf2ab4ed345a103bcf9a4e111e30654b5dd8176fe83594f53b57`
- Rust/Tokio 보정 전 기준선: `e813570837d9d08203863cf6b29044cbc803c8e05b59675c068c1032fd8086e0`
- Rust/Tokio 공개 impl 보정: `97ccc7e2200f25e87a34be2219c2296260fe6bde9a35e38b8b59d1dc2f7415f8`
- Dart/linter: `e93ee9488dab4fa040295c3bd2b060257f3ad4070421c183da40e7e3de3e7da1`
- C#/EF Core v10.0.10 최종: `a184b1db7e66ac84a9fd34398895c91d83c296031e142b076af50da0cdb9c707`
  (bundle `ffa64e47c12cbb283435908b5436e7b92eba4b46fda3c5bd96c9c93a38870ab3`)
- Java/Spring direct-call v6: `1a67ed7c05e2f94cc0de52568f87d040089677332ac7eb9783a07dc9f8d7bd24`
  (bundle `ddfc45c0a9eb3f2ceae4481788c3fc5ec50c0253d75d7b15696baceb6bcf9b5c`)

## 확인된 병목과 근본 결함

### Python

884개 파일에서 15,041개 LSP request를 보낸다. 그중 definition request 11,396개가 약 42초를 사용한다.
관계 source는 243개뿐인데 type annotation 후보가 10,468개라서, 모든 annotation identifier에 definition을
묻는 현재 계획이 cold 병목이다. 요청을 줄이는 shadow는 실제 canonical digest가 같을 때만 채택한다.

### C / C++

clangd의 `(anonymous enum)`, `(anonymous namespace)` label을 일반 qualified-name 축약 규칙에 넣으면 빈 이름이
됐다. anonymous label을 원자적 provider 이름으로 보존하도록 수정했다. 또 provider가 자기 자신을 parent로
보내는 불가능한 containment는 선언 자체를 버리지 않고 해당 parent만 제거한다. libuv와 fmt에서 cold/warm
digest 일치를 확인했다.

### JavaScript / TypeScript

ESLint는 작은 AnalysisPlan root 20여 개마다 scip-typescript runtime을 다시 시작해, 1~10개 파일짜리 unit도
약 32~36초를 썼다. cross-root multi-config 한 프로세스는 전체 시간을 51% 줄였지만 다음 사실을 누락했다.

- `tests/lib/types/types.test.mts:2257`의 `CustomParserServices.program`
- 위 field의 `Declares` 구조 관계 1개
- 위 field에서 `CustomParserServices`로 기록된 provider type 관계 1개
- source definition/type evidence 각 1개

`--no-global-caches`를 켜도 결과는 동일하게 누락됐다. 원인은 단순 process-global cache가 아니라 서로 다른
analysis root/config program을 한 execution root로 접은 데 있다. 따라서 이 batching은 환경 변수
`CODE_MEMORY_EXPERIMENTAL_TS_MULTI_CONFIG_BATCH=1`을 명시한 shadow에서만 실행되며 기본 경로는 정확성
기준선을 유지한다.

NestJS에서는 두 개의 별도 정확성 결함을 실제 파일 소유권으로 확인했다.

- TypeScript `Program`의 transitive source 전체를 해당 config의 직접 member로 기록해, integration config가
  `packages/core` 같은 다른 package 파일을 먼저 차지했다. 직접 member는
  `parseJsonConfigFileContent().fileNames`로 한정하고 transitive source는 증거 수집에만 사용한다.
- language cache key에 planned scope와 실제 provider config 내용이 없어서, 같은 파일 집합이 과거의 다른
  tsconfig 결과를 재사용할 수 있었다. 두 입력을 모두 cache identity에 포함했다.

보정 후 `sample/01-cats-app/src/cats/cats.controller.ts`에서 controller, field, GET/POST route,
`findAll -> CatsService.findAll`, DTO/type import의 정확한 file/line evidence를 사람이 대조했다.

최종 감사에서는 프레임워크 투영과 source coordinate 경계도 추가로 검증했다.

- NestJS 통합 테스트의 `request(server).get(...)`/`supertest(app).post(...)`가 Express 또는 Fastify 서버
  등록으로 잘못 승격됐다. 호출식의 인자를 receiver로 읽지 않고, 실제 `express()`/`Fastify()` 생성 또는
  해당 framework import가 증명된 bare receiver만 등록으로 인정하도록 수정했다. Express endpoint 279개와
  Fastify endpoint 24개는 각각 실제 benchmark route 1개로 줄었고 Nest route 280개는 유지됐다.
- scoped npm package의 `@`를 LSP 위치 suffix로 오인해 display name을 `npm`으로 자르던 규칙을 exact
  `@line:column` suffix에만 적용했다. 최종 bundle의 `@nestjs/*` symbol 4,445개에서 `npm`/`ts\`` 잘못된
  표시명은 0이다.
- scip-typescript가 파일마다 내는 `0:0-0:0` document sentinel 중 일부가 Namespace로 분류돼 최종 노드가
  되었다. 길이 0 provider definition은 verified source definition이 아니므로 노드로 승격하지 않고, 해당
  sentinel을 향하는 provider relation도 unresolved 사실로 오인하지 않게 명시적으로 폐기한다.

NestJS 최종 fresh cold는 155.038초(provider 81.654초), 동일 입력 warm은 15.395초와 14.775초였다.
cold/warm semantic digest `23306b38...d2c4e`, bundle digest `7b1fefa6...a2940`, nodes 6,907 /
edges 18,207 / evidence 20,019 / gaps 2,202가 정확히 같다. raw 156개 artifact의 document 1,728,
occurrence 138,862, relation 48,011을 검사해 범위 오류·빈 relation range가 0이고, 최종 source evidence
18,291개는 digest/line/UTF-8 column/byte offset 오류와 zero-length가 모두 0이다.

ESLint 최종 fresh cold는 364.736초(provider 310.148초, discovery/model 43.795초), warm은 7.980초였다.
canonical semantic digest `d3432da9...4731`, bundle `6596eb07...00a5`, nodes 2,624 / edges 4,957 /
evidence 6,622 / gaps 1,302가 cold/warm에서 같다. `ESLint.isPathIgnored -> calculateConfigForFile`,
`calculateConfigForFile -> ConfigLoader.loadConfigArrayForFile`, `lintFiles -> findFiles`를 원본과 SQLite
call-site byte offset으로 대조했다. 최종 source evidence 5,166개는 오류·zero-length 0이다.

단, JavaScript raw provider 결과에는 두 종류를 분리해 남긴다. 빈 fixture 파일 18개의 `0:0` document
sentinel은 실제 정의가 아니며 canonical에서 안전하게 제외된다. 별도로 non-BMP 이모지 property key 8개는
scip-typescript 0.4.0이 UTF-16 code-unit column을 내보내 UTF-8 boundary와 맞지 않는다. 현재 canonical은
이를 억지로 보정하지 않아 잘못된 confirmed 사실은 0이지만 해당 property 정의는 빠진다. scip-typescript
경계의 version-gated UTF-16→UTF-8 정규화와 TS/JS 재감사는 후속 gate다.

### Go

Prometheus에서 gopls는 40,566 requests를 사용했다. 그중 definition이 31,502개지만 실제 provider wall 합은
약 2.17초라서, Python과 달리 단순 request 수가 주 병목은 아니다. gopls session 준비와 semantic index가
약 83.4초, provider stage가 약 147.8초였다. `cmd/prometheus/main.go`의 `main`과
`notifier.NewManager`, `discovery.NewManager`, `scrape.NewManager`, `tracing.NewManager` 호출을 실제 소스와
SQLite evidence로 대조했다.

분석 중 config 없는 JS/TS 보조 shard가 `scip-typescript --infer-tsconfig`로 선택한 저장소 안에
`tsconfig.json`을 생성하는 결함도 발견했다. config 없는 파일은 이제 provider 작업 디렉터리의 격리된
source-only config로만 실행하며 선택한 source tree를 쓰지 않는다.

### Rust

Tokio 기준선은 rust-analyzer 173.849초가 cold의 대부분이다. type position 19,745개를 definition/type
질의 대상으로 만들었고, 반대로 call input은 0이었다. 원인은 대형 workspace 경계 규칙이 `pub fn`이어도
impl 안에 들여쓰기된 메서드를 제외한 것이었다. provider 원본에는 `Runtime::spawn`,
`Runtime::spawn_blocking`, `Runtime::block_on` 정의가 정확히 있었지만 canonical graph에는 호출 관계가
없었다. 공개 impl 메서드도 provider-backed map boundary로 유지하도록 수정하고 Rust 전용 cache marker와
회귀 테스트를 추가했다.

보정 run은 오히려 194.993초로 기준선보다 약 9.5초 빨랐고 raw provider relation이 212개에서 275개로
63개 늘었다(기존 relation 제거 0). canonical에서는 node +36, edge +73, evidence +73이며 cold/warm digest가
같다. `tokio-util/src/sync/cancellation_token.rs`에서
`run_until_cancelled_owned -> run_until_cancelled -> {is_cancelled, cancelled}`를 실제 소스 304/308/334행과
SQLite call-site evidence로 대조했다. 기본 feature context에서 inactive인 `Runtime::spawn` 자체는 관계를
강제로 만들지 않았다. 공개 메서드를 모두 포함해도 provider 시간은 173.526초로 거의 늘지 않았고, 남은
병목은 57,511 definition requests다.

### Dart

linter checkout에는 `.dart_tool/package_config.json`이 없었다. 따라서 선언 6,121개와 정적 import/type
관계는 만들었지만 외부 package를 요구하는 호출·상속은 `missing-dependency`로 남았다. 엔진 결함과 metadata
부족을 분리하기 위해 package config가 있는 `native-lsp-dart` fixture를 실제 provider로 실행했고,
`main -> add`, `main -> Box.get`, `main constructs User/Box`, `User implements Entity`,
`Box uses_type Entity`가 모두 exact evidence로 생성됨을 확인했다.

첫 linter 실행은 `test_data/rules/experiments/nnbd/analysis_options.yaml`을 provider가 읽었지만 AnalysisPlan이
소유하지 않아 실패했다. nested `analysis_options.yaml`은 language experiment와 analyzer semantics를 바꾸므로
실제 analysis-unit root로 승격했다. 보정 후 12 units / 592 Dart files가 성공했고 cold/warm semantic·bundle
digest가 동일했다.

또 의미 사실이 정말 0개인 작은 Dart unit을 cache writer는 저장하면서 reader가 무조건 버려 매 warm run마다
language server를 다시 띄웠다. `EmptySemantic` 완료 receipt가 있는 빈 결과만 재사용하고 timeout/일반 실패는
계속 재실행하도록 구분했다.

### C#

EF Core `main`은 고정 시점에 .NET 11 preview SDK를 요구했지만 bundled SDK는 10.0.301이었다. 이를 임의로
낮춰 분석하지 않고, SDK가 실제로 맞는 EF Core v10.0.10 `db55508`을 정확도 corpus로 고정했다. 5,536개
C# 파일의 scip-dotnet 인덱스는 약 171MB이며 provider definition identity는 88,205개다.

처음 canonical graph에서는 `DbContext.SaveChanges()`의 호출 source가 메서드가 아니라 `DbContext` 클래스에
붙었다. scip-dotnet occurrence의 `enclosing_range`가 비어 있고 표현식 본문 메서드는 중괄호 기반 fallback으로
소유자를 찾을 수 없었기 때문이다. source CST에서 callee 위치와 가장 가까운 정확한 실행 소유자 이름 범위를
수집하고 SCIP definition range index로 결합했다. 보정 후 다음 사실을 원본과 SQLite에서 직접 대조했다.

- `src/EFCore/DbContext.cs:597` — `SaveChanges()` → `SaveChanges(bool)`
- `src/EFCore/DbContext.cs:648` — `SaveChanges(bool)` → `IStateManager.SaveChanges()`
- 비동기 overload도 원본 732행·790행과 일치
- 모든 간선은 `confirmed`이며 producer `scip-dotnet 0.2.14`, 정확한 call-site span을 가진다.

또 `IInfrastructure<IServiceProvider>`를 AST가 안쪽 `IServiceProvider`로 잘못 읽어 provider의 올바른 hierarchy
관계를 버리고 있었다. 공통 타입 이름 추출기에서 `type_argument_list`를 이름 후보에서 제외했다. 보정 후
`DbContext`의 원본 base list 4개(`IInfrastructure`, `IDbContextDependencies`, `IDbSetCache`,
`IDbContextPoolable`)가 모두 confirmed `implements`로 남는다.

성능은 정확도와 별개로 세 병목을 계측해 제거했다.

- provider relation dedupe가 25만 관계마다 전체 누적 결과를 다시 훑던 O(N²)이었다. 동일 identity group만
  insertion order로 조회하도록 바꿨고 legacy first-match 결과와 회귀 테스트로 대조했다. warm merge는 2.1초다.
- 정의·타입 수집은 AST 노드마다 모든 조상을 반복 탐색했다. 재귀 순회 상태를 자식에게 전달해 의미 규칙을
  유지한 채 O(nodes × depth)를 O(nodes)로 바꿨다.
- 정의 수집 80.862초 → 5.953초, type-use 27.621초 → 3.981초, source inventory 128.133초 →
  28.533초, 전체 warm 266.992초 → 162.931초로 줄었다.

이후 실제 파일 coverage를 다시 대조하자 5,534개 C# 파일 중 provider가 반환하지 않은 360개는 분석 실패가
아니었다. 네 functional-test 프로젝트가 해당 `Scaffolding/Baselines/**`를
`<Compile Remove="...">`로 명시적으로 빌드에서 제외하고 있었다. literal·unconditional MSBuild remove만
스케줄 단계에서 해석하고 조건식·property expression은 추측하지 않도록 했다. 최종 coverage는 active C#
5,174개 `indexed`, build 제외 360개 `excluded + missing_compile_context`, Python 보조 파일 2개 `indexed`다.

또 scip-dotnet 0.2.14가 Roslyn의 UTF-16 column을 SCIP의 UTF-8 byte column처럼 전달한다는 경계 결함을
`Entityß` 정의에서 확인했다. 모든 C# occurrence와 enclosing range를 source line 기준 UTF-16→UTF-8로
정규화한 뒤 cache marker를 올렸다. raw occurrence 1,761,195개, 전역 정의 215,986개, range가 있는 관계
467,927개를 원문 바이트와 전수 대조해 범위 오류·UTF-8 중간 절단·빈 전역 정의·빈 관계 범위가 모두 0임을
확인했다. canonical source evidence 322,032개도 content digest, line, UTF-8 column, byte offset이 전부
원문과 일치했다.

최종 빈 제품 cache cold는 483.563초이며 scip-dotnet+변환은 308.052초였다. 같은 입력 warm은
168.572초이고 provider 실행은 0ms다. cold와 warm 2회의 semantic digest
`a184b1db...c707`, bundle digest `ffa64e47...0ab3`, nodes 72,800 / edges 221,856 /
evidence 327,568 / gaps 67,989가 정확히 같다. confirmed-without-evidence, dangling source/target는 모두 0이다.

제한도 숨기지 않는다. EF Core의 비공개 Azure NuGet feed가 401이라 `eng/common/internal/Tools.csproj`와
이를 참조하는 일부 테스트 project model은 완전하지 않다. 또한 tree-sitter-c-sharp가 preprocessor가 섞인
fluent expression과 identifier로 쓰인 `async` 등을 못 읽어 독립 definition inventory 490개 파일,
test inventory 434개 파일이 typed gap이다. provider-backed 핵심 정의·호출은 유지되며 위 `SaveChanges`
흐름도 확인됐지만, 이 보조 test 연결 수치를 전체 테스트 정확도로 홍보하지 않는다.

### Java

Spring Framework 첫 고정 cold는 합격 결과를 만들지 못했다. build-backed JDTLS는 8,982 Java 파일에 대해
`textDocument/documentSymbol` 8,982건을 수행했지만 semantic fact 0개였고, source-only fallback은 1GB heap
OOM 뒤에도 CPU를 계속 사용했다. 두 현상을 하나의 "Java가 느림"으로 묶지 않고 provider log까지 내려가
다음 원인을 분리했다.

- 격리 provider workspace에 `buildSrc/config/checkstyle/checkstyle.xml`이 없어서 Gradle project import가
  Checkstyle 구성 단계에서 실패했다. Source Census가 이 파일을 Unsupported로 기록했고 workspace copier가
  Included만 복사한 것이 원인이다.
- source-only fallback은 8,982개 문서를 모두 열어야 definition denominator를 보존하는데 Windows launcher가
  고정 `-Xmx1G`였다. `.metadata/.log`에서 `java.lang.OutOfMemoryError: Java heap space`와
  `Publish Diagnostics` stack을 확인했다.

분석 사실 범위를 줄이지 않고 provider execution fidelity를 고쳤다. regular·non-sensitive support file을
seal/digest한 격리 복사본에 포함하고, JDTLS heap을 파일 수와 가용 memory budget으로 계산하며, 제품에서
사용하지 않는 editor diagnostics만 차단했다. Spring 조건의 계산 heap은 4,018MB다. 새 cold/warm과 사람 원본
대조가 끝날 때까지 Java 상태는 **보정 중**이며, 이전 693초/162초 수치는 최종 표에 쓰지 않는다.

보정 release로 수행한 첫 완전 실행은 canonical bundle을 만들었지만 provider가 정확히 1,800초 session limit에
도달해 `indexed-partial`이었다. 결과는 nodes 64,847 / edges 127,745 / evidence 129,682 / gaps 25,006,
semantic `1f77989d...c4113`이며 최종 기준선으로 채택하지 않는다. 단계 계측 결과 JDTLS의 실제 request wall
합보다 약 24분 긴 시간이 결과 조립 내부에서 발생했다. 대형 workspace가 사전 선택한 definition 위치만
조립 단계에서 소비하도록 제한을 양쪽에 강제했고, fresh cold/warm 재측정으로 사실 보존을 검증 중이다.

계획 강제판은 cold 472.081초, warm 104.253초로 끝났고 cold/warm semantic·bundle digest가 동일했다.
`enrichment`는 1,717.228초에서 215.162초로, JDTLS 전체는 1,800.879초에서 301.617초로 줄었다. dangling과
confirmed-without-evidence는 모두 0이다. 그러나 사람 검증에서 호출 선택의 새 정확도 결함을 발견했다.
4,096개 call-hierarchy 예산을 `(우선순위, 파일 URI)`로 전역 정렬한 뒤 앞에서 잘라 Spring의 call source
4,328개가 `spring-beans` 3,158 / `spring-aop` 1,035 / `spring-context-support` 112 /
`spring-context-indexer` 22 / `buildSrc` 1에만 몰렸다. `spring-webmvc`의 핵심
`DispatcherServlet.doDispatch()`는 최종 graph에 없었다. 따라서 결정성·시간 통과만으로 Java를 합격시키지
않으며, 전 모듈 대표성을 보장하는 exact call-site definition shadow와 원본 대조를 다음 gate로 둔다.

호출 대표성 보정을 위해 concrete Java syntax에서 505,213개 실제 호출 위치와 정확한 실행 owner를 수집하고,
JDTLS `textDocument/definition`으로 저장소 내부 target만 확정하는 direct-call shadow를 실행했다. 78,513개
질의에서 49,071개가 local target으로 해석돼 raw provider 호출·생성 관계는 48,945개가 됐고, 기존
call-hierarchy 기준선의 6,603개보다 폭이 크게 늘었다. `spring-webmvc`도 5,802개 관계를 가져
`DispatcherServlet.doDispatch`에서 `getHandler`, `getHandlerAdapter`, `handle` 등의 정확한 source evidence가
생겼다.

그러나 이 v1 결과는 전수 source-token 대조에서 48,945개 중 157개가 target 이름과 불일치했다. 원인은
JDTLS가 반환한 전체 선언 범위 안에서 가장 작은 nested symbol을 고르던 client range matcher였다. 정확한
selection/full declaration 우선 규칙으로 고치고 공통 provider cache contract를 v153으로 올렸다. 또
alphabetical file prefix 편향을 막기 위해 각 파일의 최우선 호출을 먼저 배정하고, local 선언 이름이 전혀
없는 호출은 저장소 내부 관계 질의에서 안전하게 제외했다. v1은 결함 발견용 invalid shadow다.

첫 range 보정본 v2는 실제 의미 질의를 시작하기 전에 Java worker가 `capacity overflow`로 panic했다. 호출
후보 정렬의 `usize::MAX` sentinel을 `Vec::reserve` 크기로 사용한 것이 원인이며, Java definition 0인 상태에서
보조 JavaScript/Python 사실만 담은 bundle을 만들었으므로 수치 전체를 폐기했다. sentinel 기반 예약을 제거하고
무제한 정렬 회귀 테스트를 추가했다. v3에서 panic 재발 없이 Java provider와 canonical bundle 생성까지
완료되어 이 메모리 결함 자체는 닫혔지만, 아래의 별도 target 정확도 결함 때문에 v3 결과는 폐기했다.

v3은 provider와 canonical 무결성 gate를 통과했고 확정 호출도 52,128개로 늘었지만, raw 호출·생성 51,990개
전수 source-token 대조에서 `new ClassEmitter(v) -> setTarget()` 오답 1개가 남았다. JDTLS가 selection `0:0`,
선언 47~55행이라는 불가능한 symbol을 보냈고 client가 이를 exact match로 신뢰한 것이 원인이다. selection이
자기 선언 범위 밖이면 target 후보에서 제외하도록 고치고 공통 cache contract를 v154로 올렸다. v3도 최종
기준선에서 제외했고 v4 검증을 시작했다.

v3의 대표 `DispatcherServlet.doDispatch` 경로를 raw provider와 canonical SQLite로 대조하자 raw에 있던
`processDispatchResult`, `triggerAfterCompletion`이 canonical에서 빠져 있었다. JDTLS method label 전체 길이를
정의 name range로 사용해 긴 signature가 source line 밖으로 나가고 정의가 탈락한 것이 원인이다. Java 정의
근거는 source name token 길이만 사용하도록 고쳤고, malformed selection은 선언 안의 고유 source name으로만
복구한다. 이미 결함을 포함한 v4는 조기 중단했다.

v5는 cold 590.804초에 완주했고 callable/type 정의 evidence 111,097개를 원본 source token과 전수 대조해
범위 오류와 이름 불일치가 모두 0임을 확인했다. 하지만 performance receipt의
`rejectedMalformedJavaSymbols=2524`를 별도 검사하자 JDTLS가 정상 selection과 선언 시작을 주면서 선언 끝만
`0:0`으로 보내는 사례가 대량 포함돼 있었다. 실제 v3 원본에서 선언 밖 selection 2,787개 중 2,780개는
정확한 Java identifier였으며, `HeadersState`, `HeadersPredicate`, `DispatcherServlet.doDispatch` 같은 정상
정의와 그 호출 흐름이 v5에서 빠졌다. 따라서 오답 0만으로 v5를 합격시키지 않는다.

선언 범위가 실제 source 좌표로 유효하지 않고, selection이 provider가 준 선언 시작 이후이며, 그 위치의
source token이 provider base name과 정확히 같을 때만 선언 끝을 name token까지 복구한다. 정상 선언 범위
밖의 selection은 기존처럼 선언 안의 이름이 고유할 때만 옮기고, `setTarget@0:0` 같은 불가능한 좌표는 계속
거부한다. Java cache marker를 `java-definition-name-evidence.v2`로 올렸고 LSP 테스트 39개가 통과했다.
v6는 cold 639.897초, 같은 cache warm 122.043초에 완료됐다. repaired symbol은 3,451개,
rejected symbol은 215개로 줄었고, syntax definition 129,570개 중 129,418개를 provider 정의와 일치시켰다.
raw CALLS/CONSTRUCTS 51,982개와 callable/type definition 113,406개를 원본 source에 전수 대조해 잘못된 범위,
없는 파일, target-name 불일치가 모두 0임을 확인했다. `DispatcherServlet.doDispatch`는 20개 raw 호출을 가지며
`processDispatchResult`, `triggerAfterCompletion`도 canonical SQLite에서 confirmed로 이어진다.
cold/warm semantic digest와 bundle digest, node/edge/evidence/gap 수가 모두 같고 dangling endpoint와
confirmed-without-evidence도 0이므로 이 corpus의 Java 검증은 통과한다. 남은 215개 provider 좌표는 이름을
고유하게 증명할 수 없어 관계를 만들지 않은 의도된 abstention이다.

## 이미 적용한 안전한 공통 개선

- provider가 끝난 뒤 framework/출력 단계에서 실패해도, 동일 입력 재시도는 완료된 provider cache를 재사용한다.
- provider normalization cache key를 전체 executable hash에서 명시적 normalization schema로 분리했다. UI,
  linker, 저장 코드 변경이 수분짜리 provider 결과를 불필요하게 지우지 않는다. normalization 의미가 바뀌면
  schema를 반드시 올린다.
- TypeScript/JavaScript의 미소유 fallback 파일은 다른 모든 config를 재실행하지 않고 격리 source-only
  config에서만 처리한다.
- 여러 provider shard receipt는 하나를 덮어쓰지 않고 Composite execution context로 합친다.
- source dependency, framework, architecture cache가 전체 executable hash에 묶여 있어 어떤 Rust 코드 rebuild도
  모든 언어 cache를 우회하던 경로를 명시적 contract version으로 분리했다. census/planner/framework/projection
  의미가 바뀔 때만 해당 version을 올린다.

## 다음 실행 순서

1. TypeScript/NestJS와 JavaScript/ESLint의 config·package 소유권 및 source evidence 전수 재검증
2. Go/Prometheus, Rust/Tokio, Dart/linter의 build context·원문·cold/warm 재검증
3. C/libuv와 C++/fmt의 compile database·header·overload 경계 재검증
4. Python definition request 축소와 공통 canonical/linker 최적화는 exact digest shadow로만 평가
5. 전체 gate, clippy, fmt, 10언어 최종 판정표 갱신
