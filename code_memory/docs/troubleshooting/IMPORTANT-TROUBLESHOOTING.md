# Code Memory 중요 트러블슈팅 기록

상태: **운영 중인 누적 문서**

마지막 갱신: 2026-08-09
범위: 제품 정확도, 데이터 계약, provider 실행, packaging, 대규모 성능처럼 다시 발생했을 때
사용자 가치나 출시 판정을 바꿀 수 있는 문제만 기록한다.

## 기록 원칙

모든 경고와 사소한 수정 로그를 쌓지 않는다. 다음 중 하나에 해당할 때만 이 문서를 갱신한다.

1. 잘못된 사실이나 관계를 `confirmed`로 보여줄 수 있는 문제
2. 지원 언어·DB·framework·provider 계약이 코드와 문서 사이에서 달라진 문제
3. 테스트는 통과하지만 실제 설치본·사용 경로는 실패하는 문제
4. source coverage, 결정성, 대규모 처리, 보안 경계를 바꾸는 문제
5. 같은 종류의 재발을 막는 공통 불변식이나 점검 순서가 생긴 문제

각 항목은 `증상 → 영향 → 잘못 짚기 쉬운 원인 → 근본 원인 → 수정 → 검증 → 재발 시 점검`을
남긴다. 수치가 없는 “해결됨”은 쓰지 않는다.

---

## TS-2026-08-07-01 — Ruby/PHP active support hard cut

### 결정

제품의 active code-language contract를 12개에서 다음 10개로 줄였다.

`TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust, Dart`

Ruby와 PHP는 숨김 처리나 beta 표시가 아니라 provider 실행, 공용 enum, analysis planning,
framework pack, fixture, quality gate, provider manifest와 설치 archive에서 제거한다.

### 증상

- Ruby ground-truth fixture는 실제 call 5개를 모두 놓치고 false call 5개를 출력했다.
- PHP fixture는 project-local call 5개 중 chained method 2개를 놓쳤다.
- 제품은 모든 active 언어에 같은 evidence·coverage·failure 계약을 요구하지만, 두 provider를 그
  기준까지 근본 보정하려면 별도 parser/semantic fallback과 framework metaprogramming 정책이
  필요했다.
- 언어 registry 한 곳만 지우면 planner, LSP/SCIP runner, framework pack, release archive에 전용
  코드와 바이너리가 남는 구조였다.

### 영향

지원한다고 표시하면서 정확한 관계를 제공하지 못하거나, 설치 파일에 쓰지 않는 provider 약
527MB가 계속 포함될 수 있었다. 평균 점수는 전체 문제를 숨길 수 있었고 weakest-language trust는
0/100이었다.

### 잘못 짚기 쉬운 원인

“Ruby와 PHP는 동적 언어라 원래 불가능하다”가 아니다. JavaScript와 Python도 동적 언어지만 active
contract에 남아 있다. 이번 결정의 원인은 **현재 provider 신뢰도와 근본 보정 비용이 동일 품질 기준에
맞지 않은 것**이다. 언어의 동적 성질은 남는 gap을 설명하지만 지원 제외의 단독 기준이 아니다.

### 근본 원인

지원 범위가 다음 여러 authority에 중복 기입되어 있었다.

- `ProgrammingLanguage` enum과 `LANGUAGES` registry
- AnalysisPlan marker/context match와 provider schedule
- SCIP/PHP source-only fallback, Ruby LSP 실행 옵션과 diagnostic
- architecture import/package 보정
- framework catalog, adapter catalog, 13개 PHP/Ruby framework pack
- provider·framework·ground-truth PowerShell gate
- managed provider manifest, signed provider catalog와 archive
- 계약 문서와 제품 요구사항

따라서 registry만 바꾸는 수정은 유령 지원을 남긴다.

### 적용한 수정

1. 공용 `ProgrammingLanguage`와 실행 registry를 10개 closed ID로 변경했다.
2. PHP Composer source-only workspace와 scip-php 전용 실행/경로 rewrite를 제거했다.
3. Ruby LSP bundle cache, launcher, warning, implicit member-call 보정을 제거했다.
4. AnalysisPlan, execution context, context dimension에서 Composer/Bundler marker를 제거했다.
5. architecture의 PHP namespace index, Ruby/PHP import·package·dynamic marker, Composer/Gemfile
   metadata와 Rails 전용 DB asset 분류를 제거했다.
6. PHP/Ruby framework adapter 분기와 13개 framework pack을 제거해 85개에서 72개로 줄였다.
7. PHP/Ruby fixture와 ground-truth case를 제거했다.
8. provider manifest와 packaging catalog에서 두 provider를 제거했다.
9. 지원 계약과 정확도 문서를 10개 언어 기준으로 갱신했다.

`.ruby-lsp` 디렉터리 ignore 규칙은 source scanner가 사용자 저장소의 generated/cache 영역을 읽지
않게 하는 일반 hygiene 규칙이므로 남긴다. 이 ignore는 Ruby 분석 지원이나 provider 실행을 뜻하지
않는다.

### 작업 중 발견한 중요한 문제

#### A. enum을 먼저 닫자 17개 유령 분기가 컴파일 오류로 드러남

문자열 검색만으로 지우지 않고 공용 enum에서 `Php`/`Ruby`를 먼저 제거했다. 그 결과 planner,
execution-context, diagnostic에 남은 17개 match branch가 컴파일 오류로 드러났다. closed enum을 먼저
줄이고 compiler error를 제거하는 순서가 지원 범위 hard cut의 재발 방지 절차다.

#### B. 컴파일 통과 후에도 테스트가 12개 count를 truth처럼 들고 있었음

엔진은 컴파일됐지만 direct/donor IR parity와 execution-context 테스트 3개가 `12`를 기대해 실패했다.
fixture helper의 언어 목록은 10개였으므로 테스트 이름·unit/file/definition/relation/context count를
10개 계약으로 함께 바꿨다. 개수 assertion은 지원 범위 authority와 반드시 같은 변경에 포함한다.

#### C. source runtime을 지워도 release 설치본에는 남을 수 있음

`code_memory/providers/php`, `code_memory/providers/ruby`만 지우면 기존 `providers-php.zip`,
`providers-ruby.zip`, signed catalog, core manifest에는 두 provider가 남는다. source manifest를 먼저
바꾸고 provider asset script로 catalog·signature·archive를 다시 만드는 것이 완료 조건이다.

#### D. 당시 정확도 상승은 문제 해결이 아니라 측정 범위 변경이었음

12개 baseline의 micro F1 77.78%, weakest 0에서 10개 baseline의 micro F1 88.89%, weakest 50으로
바뀌었다. 이는 Ruby/PHP 결함을 고친 수치가 아니라 active denominator에서 제외한 결과였다. 당시 남은
C, C++, C#, Go, Rust 오류로 strict release gate가 실패했고, 아래 후속 항목에서 별도 근본 수정했다.

### 당시 검증 결과 (relation reconciler 구현 전)

| 검증 | 결과 |
| --- | --- |
| Code Memory Rust unit tests | 215 passed, 0 failed |
| shared fact-model contract tests | 14 passed, 0 failed |
| framework pack gate | 72/72 passed |
| signed offline provider pack | 8개, catalog/signature 검증 통과 |
| clean-install provider bundle gate | 10/10 languages, skipped 0 |
| release `list` / `doctor` | exact 10 languages / READY 10/10 |
| active ground-truth cases | 10개, cold/warm deterministic 10/10 |
| reviewed source coverage | 24/25 = 96.00% |
| project-local CALLS | TP 32, FP 5, FN 3 |
| precision / recall / micro F1 | 86.49% / 91.43% / 88.89% |
| weakest-language trust | 50/100 |
| strict accuracy release gate | 당시 실패 |

정답표와 상세 language별 수치는
[`SEMANTIC-QUALITY.md`](../contracts/SEMANTIC-QUALITY.md)를 기준으로 한다.

### 재발 시 점검 순서

지원 언어를 추가하거나 제거할 때 다음 순서를 지킨다.

1. `ProgrammingLanguage` closed enum과 `LANGUAGES` registry
2. `cargo check`로 exhaustive match 잔여 분기 수집
3. planner/context/scheduler와 provider runner
4. framework catalogs/packs 및 fixture
5. ground-truth manifest와 모든 language-count assertion
6. managed provider source manifest
7. signed provider archive/catalog 재생성
8. `doctor`, Rust tests, framework gate, cold/warm ground-truth gate
9. 계약 문서와 제품 요구사항의 active scope
10. 전체 저장소에서 제거 언어/provider 이름을 검색하고, 남은 항목이 역사 기록·negative assertion·
    source hygiene 중 하나인지 각각 설명

### 재도입 조건

Ruby나 PHP는 코드 몇 줄을 되돌려서 재활성화하지 않는다. 다음을 모두 갖춘 별도 promotion 작업으로만
다시 추가할 수 있다.

- 공용 10개 언어와 같은 closed evidence/coverage/failure 계약
- positive, negative, missing-context, determinism, large-file fixture
- 수동 ground truth에서 precision/recall/coverage/evidence/determinism 100%
- framework pack 자체 conformance
- signed provider packaging과 clean-install gate
- 실제 중·대형 repository 표본 검증

---

## TS-2026-08-07-02 — F1 88.89%·weakest 50을 만든 call relation 구조 결함

### 증상

10개 언어를 active contract로 줄인 뒤에도 closed ground truth가 TP 32, FP 5, FN 3,
source coverage 24/25, micro F1 88.89%, weakest-language trust 50/100으로 실패했다.

- C#: `new Box<string>` constructor 누락
- C: header inline `box_id` 호출과 C용 `types.h` coverage 누락
- C++: constructor 선언과 변수 `box`를 call evidence로 오인
- Go/Rust: 한 call site에 concrete/contract target을 동시에 confirmed로 출력
- Rust: impl binding을 `CALLS`로 출력

### 잘못 짚기 쉬운 원인

언어별 fixture 이름을 예외 처리하거나, 중복 target 문자열만 지우거나, constructor도 계속 `CALLS`로
채점하면 현재 35건 숫자만 올릴 수 있다. 그러나 새 이름·새 문법에서 같은 문제가 반복되므로 합격이
아니다.

### 근본 원인

provider가 보내는 relation을 executable source 위치와 대조하지 않고 곧바로 confirmed로 승격했다.
source 전체의 call/construct denominator가 없었고, C와 C++가 공유 header를 하나의 provider context와
하나의 document path로 합쳤다.

### 근본 수정

1. `providers/call_sites.rs`에서 C#/C/C++/Go/Rust CST를 파싱해 실제 callee token과 construct expression을
   수집한다. regex나 fixture name은 truth를 만들지 않는다.
2. `providers/scip/reconcile.rs`에서 provider target을 syntax site에만 결합한다. source 선언 위치는
   `CALLS`가 될 수 없고, 한 site는 최대 한 confirmed target만 가진다.
3. provider가 call hierarchy occurrence를 빠뜨리면 LSP `definition`을 실제 CST callee 위치에만 보완
   질의한다. 이 경로로 C `box_id`를 복구했다.
4. C/C++ provider scope를 언어별로 분리하고 document merge key를 `(language, path)`로 바꿨다.
5. constructor를 `CONSTRUCTS`로 분리하고 Language IR adapter와 v2 정답표가 kind까지 검증한다.
6. provider location은 같지만 display identity가 다른 Go/Rust symbol은 실제 definition identity로
   canonicalize한다.
7. Java 생성자 판정은 이름 일치만 보지 않고 선언 prefix shape까지 검증한다. 따라서
   `void Box()`처럼 클래스와 이름이 같은 일반 메서드를 생성자로 승격하지 않는다.

### 재발 방지 불변식

- declaration/import/type-only 위치는 `CALLS`/`CONSTRUCTS`가 아니다.
- evidence range는 실제 callee token을 덮는다.
- 한 semantic context의 한 call site는 최대 한 confirmed executable target을 가진다.
- concrete implementation과 interface/trait binding은 `IMPLEMENTATION`으로 분리한다.
- C/C++ 공유 header는 물리 경로가 같아도 language/compile context별로 보존한다.
- ambiguous target은 추측하지 않고 fail closed한다.
- ground truth는 target뿐 아니라 `CALLS`와 `CONSTRUCTS` kind도 일치해야 한다.

### 최종 검증

| 검증 | 결과 |
| --- | --- |
| Code Memory Rust tests | 228 passed, 0 failed |
| closed executable truth | 35 expected / 35 emitted |
| TP / FP / FN | 35 / 0 / 0 |
| precision / recall / micro F1 | 100% / 100% / 100% |
| reviewed source coverage | 25/25 = 100% |
| evidence validity | 100% |
| cold/warm determinism | 10/10 = 100% |
| weakest-language trust | 100/100 |
| strict accuracy release gate | 통과 |

이 100%는 pinned closed corpus의 결과다. 임의 repository의 reflection, macro expansion, runtime dynamic
dispatch까지 100%라는 뜻이 아니다. 실제 repository 표본과 negative/metamorphic/holdout 분모는 별도로
확장한다.

---

## TS-2026-08-07-03 — Windows PowerShell 5가 정상 provider 진행 로그를 gate 실패로 오인

### 증상

동일한 release binary와 fixture가 PowerShell 7에서는 통과하지만 Windows PowerShell 5에서는 첫 분석의
`@codebase-workspace-progress` 출력 직후 `NativeCommandError`로 중단될 수 있었다.

### 근본 원인

bridge는 진행 상태를 stderr에 쓰고 최종 성공 여부는 process exit code로 알린다. Windows PowerShell 5는
native stderr를 `ErrorRecord`로 바꾸며, gate 전역의 `$ErrorActionPreference = 'Stop'`이 정상 exit code를
확인하기 전에 실행을 중단시켰다. 분석 정확도 실패가 아니라 측정기 호환성 결함이었다.

### 적용한 수정과 검증

native bridge 실행 구간에서만 error action을 `Continue`로 낮춰 stdout/stderr를 함께 수집하고,
실행 직후 `$LASTEXITCODE`를 유일한 성공 판정으로 사용한 뒤 원래 설정을 복원한다. 수정 후 Windows
PowerShell 5에서 cold/warm 2회 strict gate가 TP 35, FP 0, FN 0, F1 100%, weakest-language 100으로
통과했다.

### 재발 방지 규칙

진행·진단을 stderr로 보내는 native provider의 성공 여부를 stderr 존재로 판정하지 않는다. 반드시
exit code와 필수 output artifact를 함께 검사한다.

---

## TS-2026-08-07-04 — 정상 1MB 초과 소스가 세 계층에서 조용히 사라짐

### 증상과 영향

정상 source가 1,000,000 bytes를 넘으면 Source Census가 `oversized_file`로 제외했고, 구 compatibility
snapshot은 실제 content hash 대신 size/mtime fingerprint를 쓰면서 source body를 빈 문자열로 바꿨다.
provider 실행 전 필터도 같은 파일을 제거했다. 작은 정답 corpus가 100%여도 대규모 프로젝트의 핵심
파일이 분석에서 사라질 수 있었다.

첫 수정으로 엔진 내부 제한을 제거한 뒤 10언어 1.1MB E2E를 돌리자 8개 언어는 통과했지만 TypeScript와
JavaScript만 coverage 50%, FN 7로 실패했다. 이 실패를 단순 provider 누락으로 덮지 않고 bundled
provider source를 대조했다.

### 근본 원인

1. Census가 full read 또는 선제 제외 둘 중 하나만 선택하는 구조였다.
2. compatibility snapshot/cache가 대형 source의 실제 content identity를 보존하지 않았다.
3. provider source policy가 파일 크기를 generated/build-context 같은 실제 제외 사유와 섞었다.
4. scip-typescript 자체가 별도로 `--max-file-byte-size=1mb`를 기본 적용했다.

즉 한 상수를 지우는 문제가 아니라 Census → cache → provider → Language IR 네 경계와 외부 provider
기본값이 중복 authority를 가진 문제였다.

### 적용한 수정

1. 64 KiB bounded buffer로 전체 byte stream을 읽어 SHA-256, UTF-8/BOM/binary 상태, 전체·non-blank line
   count를 계산한다. UTF-8 character와 newline이 buffer 경계를 넘어도 whole-buffer 결과와 같다.
2. snapshot/cache는 크기와 mtime이 아니라 실제 content stream hash를 사용한다.
3. 대형 source를 빈 문자열로 바꾸는 compatibility 동작을 제거했다.
4. 크기 기반 provider 선제 제외와 공용 `OversizedFile` gap variant를 제거했다.
5. scip-typescript에는 임의의 새 고정 상수가 아니라 AnalysisPlan이 실제 schedule한 source 중 가장 큰
   byte size를 `--max-file-byte-size`로 전달한다.
6. provider timeout은 `QueryBudgetExceeded`, workspace budget은 `WorkspaceBudgetExceeded` partial gap으로
   기록하며 source exclusion으로 위장하지 않는다.
7. 원본 정답 fixture를 build directory에 복제하고 각 언어 한 파일을 comment-only payload로 1.1MB 이상
   확장하는 별도 cold/warm E2E gate를 추가했다.

### 검증 결과

| 검증 | 결과 |
| --- | --- |
| shared fact-model | 14 passed, 0 failed |
| Code Memory Rust tests | 228 passed, 0 failed |
| fmt / clippy `-D warnings` / release build | 통과 |
| 정상 closed semantic gate | TP 35 / FP 0 / FN 0 / F1 100% |
| 10언어 1.1MB large-source gate | 10/10 indexed, skipped 0 |
| large-source precision / recall / coverage | 100% / 100% / 100% |
| large-source evidence / cold-warm determinism | 100% / 10·10 |
| weakest-language trust | 100/100 |

### 재발 방지 규칙과 남은 경계

- file size만으로 정상 source를 제외하지 않는다.
- provider가 실제로 시도한 뒤 실패한 결과와 제품이 의도적으로 제외한 결과를 같은 상태로 쓰지 않는다.
- provider를 교체하거나 upgrade하면 엔진 설정뿐 아니라 provider 자체 default ceiling도 large-source gate로
  다시 검사한다.
- 현재 gate는 언어별 1.1MB 유효 source와 typed resource-gap 변환을 증명한다. 수십·수백 MB tier,
  실제 provider OOM/timeout injection, 전체 workspace peak-memory 분포는 별도 scale certification으로
  확장한다.

---

## TS-2026-08-07-05 — LSP 정의 계층을 받았지만 부모를 버려 메서드가 최상위로 올라감

### 증상과 영향

실제 10언어 출력을 source와 대조하자 Python의 `Box.__init__`·`Box.get`은 부모가 없었고, Go의 receiver
method는 모두 최상위였다. Rust impl method는 `impl EntityBox<T>` 같은 비시각 pseudo-symbol을 부모로
가리켰는데 Language IR은 그 pseudo-symbol을 definition으로 받지 않아 최종 부모를 제거했다. 이 상태로
지도를 만들면 class/type를 펼쳐도 그 안의 method가 보이지 않거나 파일 최상위에 흩어진다.

### 근본 원인

1. LSP `DocumentSymbol.children`을 재귀 순회하면서 child 자체만 저장하고 parent identity를 폐기했다.
2. `workspace/symbol` fallback과 `documentSymbol`이 같은 symbol을 주면 계층 정보가 없는 중복 row가 남았다.
3. Go receiver method는 LSP상 flat symbol이며, Rust method의 직접 부모는 실제 type이 아니라 impl block일
   수 있는데 이 언어 구조를 실제 provider type으로 되접는 단계가 없었다.
4. provider마다 field를 `Variable`, file function을 `Method`, constructor를 `Method`로 주는 차이를 공용
   Language IR이 구조 문맥 없이 그대로 받아들였다.

### 적용한 수정

- hierarchical document symbol의 정확한 parent name과 selection coordinate를 flatten 전에 보존한다.
- 같은 symbol identity는 deterministic하게 한 번만 남기되 parent·detail·넓은 source range를 보존한다.
- Go의 provider-native receiver 표기와 Rust의 provider-native impl 표기를 같은 파일의 유일한 실제 type에만
  연결한다. 후보가 없거나 둘 이상이면 추측하지 않는다.
- LSP constructor kind 9를 SCIP `Constructor`로 보존한다.
- Language IR에서 type 직속 `Variable`만 `Field`로, file namespace 직속 `Method`만 `Function`으로
  정규화한다. Java/Dart constructor는 parent·name·signature 조건을 함께 사용한다.
- strict gate에 모든 emitted Method/Constructor/Field/Property의 parent가 실제 definition인지 검사하는
  공용 구조 불변식을 추가했다.

### 검증 결과와 남은 경계

- Code Memory Rust **233/233**, fmt, clippy `-D warnings`, release build 통과
- 정상·1.1MB cold/warm gate 모두 CALLS/CONSTRUCTS TP 35 / FP 0 / FN 0 유지
- 두 gate 모두 emitted visual member owner **43/43**, dangling parent 0
- 이 수치는 “나온 member의 부모가 유효하다”는 무결성 수치다. source에 존재하는 모든 definition의
  recall, wrong kind, extra definition까지 100%라는 뜻은 아니었다. 후속 독립 분모와 완료 결과는
  `TS-2026-08-08-06`에 기록한다.

### AI 책임 경계

definition, file/type ownership, constructor·field kind는 source와 compiler/LSP/AST로 증명 가능한 사실이라
AI를 사용하지 않는다. AI는 canonical Fact Graph가 완성된 뒤 인증·결제 같은 의미 영역 이름, 요약과
설명을 만드는 계층에서만 사용한다.

---

## TS-2026-08-08-06 — provider 표시 이름을 source 정의명으로 비교해 생성자·필드를 누락시킴

### 증상

독립 AST inventory를 처음 연결하자 TypeScript는 source 정의 11개 중 7개만 대응했고 생성자와
parameter-property field가 누락으로 나왔다. JavaScript 생성자 1개와 C# 생성자 1개도 같은 증상이었다.
C++에서는 반대로 같은 `BoxValue` 생성자 source 위치에 일반 ID와 generic ID 두 개가 와서 extra
definition 1개가 생겼다.

후속 anti-overfitting fixture에서 TypeScript/JavaScript의 top-level arrow binding과 class arrow field를
추가하자 단순 variable/field로 분류될 위험도 드러났다. 반대로 함수 본문 안의 nested function/class는
제품 definition 분모에 섞이면 안 됐다.

### 영향

기존 `43/43 owner` 수치는 provider가 이미 내보낸 member만 검사했으므로 이 문제를 발견할 수 없었다.
단순 definition count도 이름·kind·owner가 서로 바뀐 오류를 숨길 수 있었다. 그대로 canonical graph로
가면 TypeScript field/constructor가 사라지거나 C++ constructor가 중복되고, 해당 provider alias를 쓰는
relation endpoint까지 함께 유실될 수 있었다.

### 근본 원인

SCIP/LSP protocol은 실제 source token과 다른 descriptor를 쓸 수 있다. 실제 출력은 constructor를
`<constructor>` 또는 `.ctor`, parameter property를 `(value)`처럼 표현했다. adapter는 먼저 이 표시 이름을
source 이름과 비교했으므로 정확한 occurrence range가 있어도 대응시키지 못했다. C++ provider의 두 ID는
같은 exact source range였지만 중복 definition/alias 계약이 없었다.

### 적용한 수정

1. 10언어 source declaration inventory를 provider와 독립적으로 만든다.
2. exact source name range, LSP source point, source name 순서로 대응하며 source position을 protocol
   display spelling보다 우선한다.
3. 대응 후 display name, canonical kind, parent를 source declaration으로 고정한다.
4. source declaration이 없는 provider definition은 fail-closed extra로 남긴다.
5. 이미 대응된 동일 exact range에 같은 호환 kind로 들어온 provider ID만 alias로 인정한다. 더 짧은
   기본 ID를 deterministic하게 남기고 relation endpoint를 그 ID로 redirect한다.
6. receipt와 gate에 language별 missing/extra/alias/kind refinement/owner repair/parse failure와
   `path + kind + name + parent` digest를 기록한다.
7. TypeScript/JavaScript의 callable initializer는 source AST에서 top-level `function` 또는 type-owned
   `method`로 정규화하고, executable body 안의 local/nested declaration은 10언어 모두 분모에서 제외한다.

### 회귀 방지와 검증

- `tests/ground_truth/definitions.v1.json`이 물리 source 24개(공유 C/C++ header를 각각 세는 언어 context
  25개), 117개 정의, 55개 owner, 10언어의
  local/parameter/receiver/type-parameter 금지 예를 고정한다.
- 정상 크기 cold/warm: TP 117 / FP 0 / FN 0, kind·owner·coverage·determinism 100%.
- protocol 차이 보정은 kind refinement 33건, owner repair 9건, C++ exact alias 1건으로 투명하게 남는다.
- 언어마다 source 하나를 1.1MB 이상으로 만든 사본에서도 같은 117개 definition-set digest와 55/55
  owner가 cold/warm 동일하다.
- 같은 이름이나 가까운 range만으로 alias를 만들거나, AST inventory가 provider symbol/reference target을
  새로 만드는 수정은 금지한다.

---

## TS-2026-08-08-07 — 패키지 gate가 C/C++ 공유 header와 선택형 정답 속성을 오류로 판정함

### 증상

개발 tree에서 definition·대형 파일 gate는 모두 통과했지만, signed provider bundle을 임시 경로에 풀어
엄격 모드로 실행한 release gate는 두 번 중단됐다.

1. `native-lsp-c/types.h`가 C와 C++ document로 각각 존재하자 `duplicate document path`로 실패했다.
2. definition truth의 일부 file에 선택형 `forbiddenNames`가 없자 PowerShell `Set-StrictMode -Version Latest`
   아래에서 property access 오류가 났다.

### 영향

첫 오류를 피하려고 path 하나를 제거하면 C 또는 C++의 실제 semantic context가 유실된다. 두 번째 오류를
피하려고 모든 file에 의미 없는 빈 배열을 강제하면 truth schema의 선택성과 검증기 구현이 어긋난 채
숨겨진다. 둘 다 분석 정확도 문제가 아니라 release 검증기가 올바른 결과를 거부하는 문제였다.

### 근본 원인

semantic document identity는 단순 `path`가 아니라 `(language, path)`인데 구형 gate가 물리 경로만
grouping했다. 또한 standalone PowerShell의 느슨한 property access에 우연히 의존해, 상위 release
스크립트가 켠 strict mode를 계약으로 시험하지 않았다.

### 적용한 수정

- 중복 document는 동일 `(language, path)`일 때만 실패한다. C/C++가 공유 header를 각자 분석하는 것은
  허용하지만 동일 언어에서 같은 path가 두 번 나오면 계속 차단한다.
- `forbiddenNames`는 `PSObject.Properties`로 존재 여부를 확인한 뒤 읽는다. 언어 전체에는 최소 한 개의
  negative example을 계속 요구하되 file별 속성은 선택형으로 유지한다.

### 검증 결과

- strict mode definition gate: 10/10 언어, TP 117 / FP 0 / FN 0.
- signed catalog 검증 후 8개 provider pack을 별도 임시 경로에 풀고 offline/managed-provider 강제 실행:
  uniform core 10/10 및 definition 117/117 통과.
- C와 C++ 모두 `types.h` context를 유지했고 same-language/path duplicate는 0이다.

### 재발 시 점검 순서

1. 중복 여부를 물리 path가 아니라 canonical semantic identity로 비교했는지 확인한다.
2. standalone gate와 release parent script의 strict mode가 같은지 확인한다.
3. optional truth property를 없는 값과 빈 값으로 구분하는지 확인한다.

---

## TS-2026-08-08-08 — coordinator diagnostic이 donor에만 들어가 정상 분석이 parity 오류로 중단됨

### 증상

TypeScript compiler project model을 사용할 수 없지만 SCIP provider 자체는 정상 실행된 경로에서 direct
Language IR과 donor Language IR의 semantic fact 수는 같아도 stream record가 `70/71`, issue가 `0/1`로
달라졌다. 엔진은 다음 오류로 결과 파일을 쓰기 전에 중단됐다.

`direct provider Language IR coverage/failure parity failed`

### 영향

사실 데이터가 손상되지 않았는데도 사용자는 분석 결과를 전혀 받지 못한다. 더 나쁘게는 격리된 provider
bundle gate에서는 project model이 존재해 통과할 수 있으므로, 테스트 성공과 일반 CLI launch path가
달라지는 설치 경로 결함이 된다.

### 잘못 짚기 쉬운 원인

provider cache nondeterminism이나 TypeScript SCIP 결과 차이가 아니었다. 두 stream의 definition/relation/
evidence 수와 semantic digest는 같았고 issue record 하나만 달랐다.

### 근본 원인

`DirectLanguageIrInput`은 scheduler가 만든 provider batch의 diagnostic만 받았다. 반면 donor는
`IndexOutput.diagnostics`를 받아 provider 실행 전에 compiler project model이 만든 coordinator diagnostic까지
포함했다. 즉 같은 분석 사실을 두 경로가 서로 다른 입력 집합으로 검증했다.

### 적용한 수정

- `DirectLanguageIrInput`에 `coordinator_diagnostics`를 명시적으로 추가했다.
- direct path는 coordinator diagnostic 뒤에 provider batch diagnostic을 합쳐 donor와 같은 입력 경계를 쓴다.
- path 없는 project-model 실패 diagnostic도 direct/donor가 같은 unit issue로 만드는 회귀 테스트를 추가했다.

### 검증 결과

- 재현 CLI: direct/donor `recordCount=71/71`, `issueCount=1/1`, exact stream parity 통과.
- 새 회귀 테스트 포함 Code Memory Rust 240/240 통과.
- definition 117/117, CALLS/CONSTRUCTS 35/35 cold/warm gate 통과.

### 재발 시 점검 순서

1. provider 실행 전에 생긴 manifest/project-model/coordinator diagnostic인지 분류한다.
2. direct와 donor가 같은 ordered diagnostic 집합을 받는지 확인한다.
3. semantic payload parity와 coverage/failure stream parity를 따로 비교한다.
4. 격리된 bundle path와 일반 launch path를 둘 다 실행한다.

### 남은 한계 또는 후속 gate

이 수정은 canonical clean=incremental 동등성을 증명하지 않는다. Batch H의 clean/incremental digest gate는
별도로 구현해야 한다.

---

## TS-2026-08-08-09 — import relation 0건을 `Complete`로 보고할 수 있었음

### 증상

검토 중인 10언어 결과를 직접 집계하자 provider `IMPORTS` relation은 전부 0건이었다. TypeScript와
JavaScript만 별도 compiler project model의 legacy `file_relations`에 내부 import가 각각 1건 있었고,
나머지 언어는 그 필드도 0건이었다. 그런데 SCIP/CompilerApi capability policy는 imports를 `Full`로
선언해 빈 결과가 측정 완료처럼 보일 수 있었다.

### 영향

실제 import가 있는 파일도 새 Language IR에서는 연결 0개가 되고, coverage 화면은 이를 “import 없음”으로
오해할 수 있다. 최종 지도에서는 파일·모듈 경계 간선, reverse invalidation, 영역 경계 집계가 모두
사라지는 핵심 정확도 문제다.

### 잘못 짚기 쉬운 원인

fixture에 import가 없어서가 아니다. TypeScript, JavaScript, Python, C/C++, Rust, Dart fixture에는 명시적
내부 import/include/use가 있다. 또한 legacy architecture가 일부 관계를 그린다는 사실은 새 IR에 관계가
들어왔다는 증거가 아니다.

### 근본 원인

1. SCIP occurrence decoder는 import role도 callable owner를 찾은 경우에만 relation으로 만들기 때문에
   최상위 import를 일반적으로 보존하지 못한다.
2. TypeScript/JavaScript compiler project model의 정확한 `file_relations`는 `DirectLanguageIrInput`과
   `LanguageIrMigrationInput` 어느 쪽에도 전달되지 않는다.
3. imports receipt의 denominator가 실제 import site가 아니라 unit file 수였고, 0건도 `Full` policy 때문에
   `Complete`가 될 수 있었다.

### 적용한 수정

- provider가 우연히 내보낸 `IMPORTS`를 authority로 쓰지 않고, 10언어 source syntax의 import site를
  독립 분모로 센다.
- snapshot당 `ProjectImportIndex`를 한 번 만들고 direct/donor가 같은 `file_relations`와
  `project_model_files`를 사용하도록 실제 제품 경로에 연결했다.
- internal은 exact evidence-backed `Imports`/`Exports` IR relation, known external은 경계 계수,
  unresolved/ambiguous/invalid evidence는 typed gap으로 분리했다.
- migration receipt v3에 언어별 eligible/import/export/internal/external/unresolved/ambiguous 및
  inventory/metadata failure를 기록하고 direct/donor exact parity에 포함했다.

### 검증 결과

- Code Memory Rust 251/251, shared fact-model 15/15, fmt, check, clippy `-D warnings`, locked release build 통과.
- semantic 35/35와 definition 117/117 cold/warm digest는 변경 뒤에도 동일하게 통과.
- 기존 9개 project fixture의 v3 audit에서 eligible 10, internal 8, unresolved 2, ambiguous/invalid 0을
  실제 release binary로 확인했다.

### 재발 시 점검 순서

1. public legacy output과 새 Language IR record를 분리해서 센다.
2. import 문장이 있는 source file에서 observed site, resolved internal, unresolved, ambiguous를 각각 센다.
3. 0건일 때 denominator가 실제 0인지 inventory 실패인지 확인한다.
4. relative/absolute/package/namespace/include 문법을 언어별 정확한 build metadata로 해석했는지 확인한다.
5. comment/string, external dependency, missing target, ambiguous target negative case를 함께 실행한다.

### 남은 한계 또는 후속 gate

제품 wiring은 끝났지만 Java/C#/Go 기존 fixture에는 import site가 0개이고 전용 정답표가 없다. 다음 종료
조건은 `imports.v1` 수동 ground truth와 10언어 positive/negative/ambiguous/missing-context,
cold/warm/large-file release gate다.

---

## TS-2026-08-08-10 — receipt v3 전환 뒤 정의 release gate가 v2만 허용함

### 증상

Code Memory 단위 테스트와 clippy는 모두 통과하지만 새 release binary로 definition ground-truth gate를
실행하면 `unsupported Language IR receipt schema ...v3`로 중단될 수 있었다.

### 영향

엔진 정확도가 깨진 것이 아니라 검증기가 현재 엔진을 읽지 못하는 상태다. 이 상태를 방치하면 새 코드는
통과했는데 release 정확도 보고서는 오래된 바이너리 결과만 남는 거짓 안전이 생긴다.

### 근본 원인

import audit 필드를 넣으며 Rust receipt schema를 v3로 올렸지만 PowerShell release gate와 계약 문서의
schema 상수는 v2에 고정돼 있었다. Rust 단위 테스트는 외부 gate 스크립트의 문자열을 검사하지 않는다.

### 적용한 수정

- definition ground-truth gate의 허용 schema를 v3로 동기화했다.
- Code Memory README, Language Semantics 계약, 제품 요구사항의 current receipt schema를 v3로 갱신했다.

### 검증 결과

- 새 release binary로 definition gate 2회 통과: TP 117, FP 0, FN 0, owner 55/55, 10/10 결정성.
- semantic gate 2회 통과: CALLS/CONSTRUCTS TP 35, FP 0, FN 0, 최저 언어 신뢰도 100.
- 언어별 1.1MB gate 2회 통과: 위 정의·관계·digest 유지.

### 재발 시 점검 순서

1. receipt schema를 올리는 패치에서 Rust producer와 모든 외부 parser를 함께 검색한다.
2. 단위 테스트만으로 끝내지 않고 release binary를 만든 뒤 PowerShell gate를 실제 실행한다.
3. gate 보고서의 생성 시각과 바이너리 빌드 시각이 현재 checkout 이후인지 확인한다.

### 남은 한계 또는 후속 gate

현재 schema 문자열은 여러 소비자에 중복돼 있다. canonical bundle 단계에서는 machine-readable schema
contract/golden을 release gate가 읽게 만들어 수동 문자열 동기화를 제거해야 한다.

---

## TS-2026-08-08-11 — clean fixture의 두 번째 분석에서 SourceManifestDigest가 달라짐

### 증상

10언어 import 전용 fixture의 source 하나씩을 1.1MB 이상으로 확장한 깨끗한 임시 복사본에서 같은 release
binary를 같은 cache로 두 번 실행했다. 두 실행 모두 36개 import/export site의 outcome, target,
UTF-8/UTF-16 evidence와 언어별 summary 대조를 통과했지만 마지막 결정성 검사에서
`SourceManifestDigest`가 달라져 gate가 실패했다.

### 영향

코드가 바뀌지 않았는데 첫 분석과 다음 분석의 input receipt가 달라진다. AnalysisPlan은 manifest digest를
참조하므로 clean→warm 전체 영수증 결정성을 보장할 수 없고, 불필요한 재분석과 사용자 repository 오염도
생길 수 있다. import 사실 자체가 틀린 것은 아니지만 release 완료로 처리할 수 없다.

### 잘못 짚기 쉬운 원인

대형 comment padding, `.gitignore` 복사 누락, import resolver의 비결정성이 아니었다. `.gitignore`는 이미
SHA-256 pinned file에 포함돼 있었고 두 실행의 reviewed import 결과도 동일했다. 기존 작은 fixture gate가
통과한 이유는 그 fixture가 이전 실행의 provider 산출물을 이미 가지고 있어 첫 측정 전부터 steady state였기
때문이다.

### 근본 원인

C# provider는 project 안에 `obj/bin`, Java provider는 `.project`, `.classpath`, `.settings`, `target`을
생성했다. Source Census는 ignored/product-ignored directory의 **존재 자체**를 non-enumerated scope로
기록하고 SourceManifest digest는 scope path와 gap을 포함한다. 따라서 깨끗한 run 1의 census 뒤에 provider가
산출물을 만들면 run 2의 manifest는 다른 것이 정상이다. provider execution이 census authority의 root를
변경한 것이 문제다.

### 적용한 수정

검증 기준을 낮추거나 생성물을 미리 넣어 통과시키지 않았다.

1. `SourceManifest`가 Included로 봉인한 regular file만 cache 아래 writable workspace에 복사한다.
2. 복사 중 byte size와 SHA-256을 다시 검사해 census 이후 바뀐 입력을 provider에 보내지 않는다.
3. 원본에 산출물을 쓰던 Java/C# provider는 격리 root와 격리 config/source path만 받는다.
4. provider 결과는 원본 repository path로 다시 매핑하고, 같은 언어 unit은 결정적 turn으로 실행한다.
5. 모든 provider 종료 뒤 Source Census를 다시 실행해 최초 manifest와 다르면 mixed snapshot 게시를
   거부한다.
6. cache root가 repository 안의 `.code_memory`로 fallback하던 경로를 없앴다.
7. import gate는 baseline도 저장소 fixture를 직접 실행하지 않고 SHA-256 pinned file만 새 임시 root에
   복사한다. 과거 ignored 산출물이 clean-start 분모를 오염시키지 못한다.

### 검증 결과

- pinned-file-only baseline: 10언어, 33개 source/config, 36/36 site, internal 15, known external 7,
  unresolved 14, invalid evidence 0, cold 47,706ms / warm 436ms, 모든 digest 동일.
- clean 1.1MB: 언어별 10개 source가 최소 1,100,048 bytes이고 두 실행 545,387ms / 488,544ms에서
  36/36 site, target, UTF-8/UTF-16 evidence, Source Manifest, Analysis Plan, IR stream, semantic payload가
  모두 동일했다.
- Code Memory 254/254, fmt, locked check, clippy `-D warnings`, locked release build가 통과했다.
- 검증 보고서는 `build/import-ground-truth-clean/import-quality-report.json`과
  `build/large-source-import-isolated/import-quality-report.json`이다.

### 재발 시 점검 순서

1. 검증 fixture가 실행 전부터 `obj`, `bin`, `target`, IDE metadata로 오염됐는지 본다.
2. clean 복사본에서 run 1 전후 전체 tree를 비교해 provider가 만든 경로를 기록한다.
3. included source digest, excluded scope ledger, AnalysisPlan digest, semantic payload digest를 각각 비교한다.
4. 생성물을 ignore하거나 미리 심어 manifest 비교를 우회하지 않는다.
5. provider가 cache/sandbox mirror에서 실행되고 evidence path가 원본 repository path로 안전하게 remap되는지
   증명한다.

### 남은 한계 또는 후속 gate

정확도 차단 요소는 닫혔다. 새 provider 또는 provider upgrade가 source root를 쓰는지는 10언어 공통
post-census mutation audit가 계속 차단한다. 별도 운영 후속으로, process별 격리 root를 기준으로 생성되는
Java/.NET 보조 cache가 장기간 누적되지 않도록 수명과 garbage collection을 provider work guard에
통합해야 한다. 이는 원본 불변성·semantic determinism과 분리해 검증한다.

---

## TS-2026-08-08-12 — import ambiguity를 가짜 문법으로 만들면 gate만 통과한다

### 증상

10언어 import baseline이 internal/external/unresolved를 모두 맞혀도 `ambiguousCount=0`이라 후보가 여러
개일 때 fail-closed로 멈추는 branch를 독립적으로 증명하지 못했다.

### 영향

resolver 구현에 ambiguity 분기가 있다는 이유만으로 완료 처리하면, 실제 multi-root/module/project에서
첫 후보를 잘못 선택해 confirmed import edge를 만들 수 있다. 반대로 모든 언어에 억지 중복 파일을 넣으면
언어의 실제 compiler/project 의미와 다른 테스트에 최적화된다.

### 잘못 짚기 쉬운 원인

C# `partial` 선언 두 개는 모호한 타입 두 개가 아니라 컴파일러가 합치는 하나의 논리 타입이다. 이를
candidate 2건으로 세면 테스트 숫자는 늘지만 제품 의미는 틀린다.

### 근본 원인

기존 정답표에는 각 언어의 positive/negative/unresolved는 있었지만, dependency ordering 정보가 없을 때
실제로 복수의 유효 후보가 생기는 독립 project context가 없었다. 또한 semantic ground-truth gate는
수동으로 통과했지만 signed provider bundle gate에 연결되지 않아 CALLS/CONSTRUCTS 회귀가 출시를 막지
못했다.

### 적용한 수정

1. Python은 별도 `pyproject` roots, Java는 별도 Maven modules의 split package, C#은 서로 참조하지 않는
   별도 `csproj`에서 같은 qualified target을 제공한다.
2. 나머지 7개 언어는 실제 후보 다중성을 꾸미지 않고 missing-context/unresolved 분모를 유지한다.
3. gate는 ambiguity가 정확히 Python·Java·C#에만 있고 각 candidate count가 2 이상이며 typed
   `unresolved_target` gap으로 끝나는지 검사한다.
4. fixture는 사람이 SHA-256으로 고정한 45개 source/config만 임시 root에 복사한다.
5. provider bundle release gate가 definition, CALLS/CONSTRUCTS, import 세 독립 정답표를 모두 실행한다.

### 검증 결과

- baseline: 10언어, 45개 pinned file, 39 site, internal 15, external 7, unresolved 14, ambiguous 3,
  current release binary cold 100,988ms / warm 430ms.
- 언어별 1.1MB: 최소 1,100,048 bytes, 같은 39 site와 네 digest가 두 실행
  598,893ms / 494,665ms에서 동일. 정확도·결정성은 통과했지만 제품 성능은 미통과다.
- Code Memory 254/254, fact-model 15/15, Tauri 38/38, DB CLI 5 + core 149 통과.

### 재발 시 점검 순서

1. 중복 후보가 해당 언어 compiler/project model에서 실제로 별개 identity인지 확인한다.
2. partial/merge semantics를 ambiguity로 세지 않는다.
3. ambiguous site가 internal relation으로 승격되지 않았는지 receipt와 relation 모두 확인한다.
4. 개별 gate뿐 아니라 signed release bundle이 그 gate를 호출하는지 확인한다.

### 남은 한계 또는 후속 gate

import capability의 현재 정답 분모는 닫혔다. 다음 독립 분모는 type/extends/implements/override/uses-type이며,
실제 대규모 workspace와 OSS holdout은 별도 scale/variance gate로 남는다.

---

## TS-2026-08-08-13 — 같은 Windows 경로를 두 표현으로 비교해 Java·C# provider가 빠짐

### 증상

현재 source로 definition 정답 gate를 다시 돌리자 Java와 C#만 `provider path is outside the selected
repository` 진단을 내고, definition 24개가 누락됐다. 반면 import gate는 39/39를 통과해 한 종류의
정답표만 보면 정상처럼 보였다.

### 영향

semantic provider가 실패하면 class/function/owner와 실제 compiler 관계가 빠질 수 있다. 더 위험한 점은
source syntax import resolver처럼 별도 보조 경로가 일부 결과를 만들 경우, 전체 provider가 정상 실행된
것처럼 오판할 수 있다는 것이다.

### 잘못 짚기 쉬운 원인

실제 파일이 repository 밖에 있거나 provider fixture가 잘못된 것이 아니었다. 두 경로는 디스크에서 같은
디렉터리였고, Windows API가 반환한 문자열 표현만 달랐다.

### 근본 원인

index 경계의 `canonical_project_root`는 Windows extended-length prefix인 `\\?\\`를 제거했지만,
`ProviderWorkspace::from_manifest`와 복사 검증은 `std::fs::canonicalize`를 다시 호출해 prefix를
되살렸다. Rust의 `starts_with`와 `strip_prefix`는 경로의 의미가 아니라 component 표현을 비교하므로
`D:\\project`와 `\\?\\D:\\project`를 서로 다른 root로 판정했다.

### 적용한 수정

1. 기존 경로를 repository ownership 표현으로 바꾸는 `canonical_existing_path`를 공용 경계로 만들었다.
2. 선택 source root, provider work parent, materialized execution root, 복사 전 원본 파일 검증이 모두 이
   경계를 사용한다.
3. 회귀 테스트는 실제 index가 넘기는 것과 같은 canonical selected root를 workspace에 전달하고 root와
   file mapping이 모두 성공하는지 검사한다.
4. import 하나로 provider 생존을 추정하지 않고 definition, CALLS/CONSTRUCTS, import 정답표를 각각
   release 차단 조건으로 유지한다.

### 검증 결과

- 수정 전 현재 source definition gate: Java·C# 누락, TP 93 / FN 24.
- 수정 후 새 release binary definition gate: TP 117 / FP 0 / FN 0, owner 55/55, 10/10 결정성.
- 수정 후 CALLS 25 + CONSTRUCTS 10 = 35/35, FP/FN 0, 10/10 결정성.
- 수정 후 import baseline: 45개 pinned file, 39/39 site, internal 15, external 7, unresolved 14,
  ambiguous 3, cold 100,988ms / warm 430ms, 네 digest 동일.
- 수정 후 언어별 1.1MB run: 최소 1,100,048 bytes, 598,893ms / 494,665ms, 10/10 provider
  `indexed`, path failure 0, 39-site 결과와 네 digest 동일.
- Code Memory 254/254, fmt, locked check, clippy `-D warnings`, release build 통과.

### 재발 시 점검 순서

1. provider diagnostic에서 `outside the selected repository`를 찾는다.
2. selected root, source input, provider execution root의 Windows path prefix와 component를 함께 출력한다.
3. 한 capability가 통과했다는 이유로 provider 전체 성공을 추정하지 말고 definition·relation·import를
   각각 확인한다.
4. 임시로 문자열을 잘라 비교하지 말고 공용 canonical path 경계와 root containment 테스트를 고친다.

### 남은 한계 또는 후속 gate

이 회귀는 local Windows drive의 실제 실행 경로를 닫는다. 별도 UNC/network-share 지원을 제품 범위에
넣을 때는 verbatim UNC와 reparse-point containment를 독립 보안·정확도 fixture로 인증해야 한다.

---

## TS-2026-08-08-14 — provider와 Language IR 시간을 같은 타이머로 재면 병목을 반대로 판단한다

### 증상

언어별 source 하나를 1.1MB로 늘린 gate에서 `provider_and_scip_conversion`과
`language_ir_adapter_validation`이 모두 약 597초로 기록됐다. 이 값만 보면 10언어 provider 실행이
느린 것처럼 보였다.

### 영향

병목을 provider/LSP 튜닝으로 잘못 돌리면 정확한 데이터를 내는 언어 도구를 건드리면서도 실제 507초
비용은 그대로 남는다. 또한 direct 경로 승격 후 제거해야 할 donor 전체 재변환이 성능 부채로 드러나지
않는다.

### 잘못 짚기 쉬운 원인

분석 unit의 elapsed 합계는 병렬 작업 시간이므로 전체 wall time과 직접 같지 않다. 반대로 두 stage가
같은 숫자를 냈다고 같은 일을 뜻하지도 않는다. 실제로는 두 타이머가 같은 provider 시작점을 공유했다.

### 근본 원인

provider 완료 뒤에도 `provider_and_scip_conversion` 타이머를 끝내지 않았고, Language IR 타이머 역시
provider 전부터 시작했다. 그 뒤 direct와 donor가 같은 `validate_language_ir` 경로를 각각 호출해 source
검증, syntax inventory, IR record emission과 digest를 두 번 수행했다.

### 적용한 수정

1. provider 종료 직후 provider 타이머를 닫았다.
2. source stability, provider execution-context reconciliation, direct IR, provider merge, donor IR,
   parity에 독립 stage timing을 추가했다.
3. baseline과 언어별 1.1MB fixture를 같은 current release binary로 각각 한 번 다시 실행했다.

### 검증 결과

| 동일 10언어 fixture | 전체 | provider | direct IR | donor IR | IR 합계 |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline | 107,882ms | 106,295ms | 73ms | 8ms | 87ms |
| 언어별 1.1MB | 615,563ms | 106,027ms | 249,037ms | 258,363ms | 507,408ms |

두 실행의 provider 시간은 사실상 같고 큰 입력에서 IR 전체 변환만 증가했다. 1.1MB 실행도 10언어
39-site 결과와 target/evidence, manifest/plan/IR digest를 그대로 통과했다.

### 재발 시 점검 순서

1. stage timer의 시작·종료가 실제 소유 함수 경계와 같은지 확인한다.
2. 병렬 unit elapsed 합계와 wall time을 서로 대체하지 않는다.
3. direct/donor처럼 parity용 이중 경로가 source parse와 digest를 중복 수행하는지 확인한다.
4. 작은 fixture와 큰 fixture에서 provider와 adapter 시간을 따로 비교한다.

### 남은 한계 또는 후속 gate

현재 계측은 병목을 direct/donor IR 변환 경계까지 좁혔다. definition/import/type inventory별 내부 비용은
다음 세부 계측으로 분리한다. 구조적 해결은 direct Language IR stream을 제품 authority로 승격하고 parity가
확보된 donor 전체 재변환을 제거하는 것이다.

---

## TS-2026-08-08-15 — raw `IMPLEMENTATION`을 canonical `implements`로 일괄 변환하면 관계 의미가 깨진다

### 증상

실제 provider 출력에서 Java/C#/C++의 class inheritance, interface conformance, Rust trait method,
C# interface method가 모두 `IMPLEMENTATION`으로 들어왔다. 기존 adapter는 이 값을 전부 canonical
`implements`로 만들었다.

### 영향

지도에서 `extends`, `implements`, `overrides`가 같은 선으로 보이고, C++ 일반 함수 declaration까지
type relation처럼 보일 수 있었다. 데이터 건수는 있어도 사용자가 읽는 의미가 틀리는 결함이다.

### 잘못 짚기 쉬운 원인

provider flag 이름이 implementation이라는 이유로 언어 공통 의미도 implements라고 보면 안 된다.
SCIP/LSP의 raw 관계 종류는 protocol 운반 형식이지 제품 canonical ontology가 아니다.

### 근본 원인

adapter가 endpoint kind와 source keyword를 보지 않고 raw 문자열 하나만 매핑했다. 특히 C#의 `:`는
base class와 interface를 같은 문법 목록에 담고, C++의 `is_definition`은 declaration→implementation
방향을 쓴다.

### 적용한 수정

1. 10언어 tree-sitter syntax inventory로 explicit `extends`, `implements`, Dart `with`, Rust
   `impl Trait for Type`의 양쪽 토큰과 UTF-8/UTF-16 range를 수집한다.
2. provider가 해결한 source/target symbol이 둘 다 retained project definition일 때만 syntax intent를
   canonical kind로 승격한다.
3. C#은 resolved target kind가 interface/trait이면 implements, class/struct이면 extends로 나눈다.
4. cross-owner method pair만 overrides로 인정하고, C++ `DEFINITION`은 그 경우에만 방향을 뒤집는다.
5. 외부·미해결 endpoint, annotation/name-only 후보는 fail-closed로 버린다.

### 검증 결과

migration receipt v4 실측에서 기존 10언어 fixture는 extends 5, implements 6, overrides 3,
uses_type 8로 분리됐다. TypeScript/JavaScript/Java/C#/C++/Go/Rust/Dart hierarchy의 실제 source token과
양 끝점이 audit sample에 남고 direct/donor exact parity도 유지됐다.

### 재발 시 점검 순서

1. raw relation kind만 보지 말고 endpoint canonical kind와 source syntax intent를 함께 본다.
2. evidence가 source keyword의 target token을 가리키는지 확인한다.
3. source/target이 retained local definition인지 확인한다.
4. 일반 function prototype이 overrides/declares로 잘못 승격되지 않았는지 negative를 확인한다.

### 남은 한계 또는 후속 gate

현재 v4 receipt는 실제 결과를 볼 수 있게 한 단계다. 독립 SHA-256 pinned 사람 정답표, Dart mixes-in,
Python local inheritance, Java override, declaration-boundary uses_type negative가 release gate에 추가돼야
type capability를 완료로 판정한다.

---

## TS-2026-08-08-16 — Rust trait 구현은 method override만 나오고 type pair가 빠질 수 있다

### 증상

`impl Entity for User`에서 rust-analyzer는 `User.id -> Entity.id` method relation은 주지만
`User -> Entity` type relation은 hierarchy API로 주지 않았다. syntax inventory에는 explicit site가
1개인데 matched site는 0이었다.

### 영향

메서드 수준으로 내려가면 일부 연결이 보이지만 상위 구조 지도에서는 `User implements Entity`가 빠져
trait 기반 설계를 읽을 수 없다.

### 잘못 짚기 쉬운 원인

`impl` 문장의 문자열에서 `User`와 `Entity`를 이름으로 찾아 연결하면 동명이인, import alias,
외부 trait에서 거짓 양성이 생긴다. method 관계를 type 관계로 올리는 것도 generic/blanket impl에서
잘못될 수 있다.

### 근본 원인

Rust의 type hierarchy provider 응답이 trait conformance를 보장하지 않았고, 기존 runner는 explicit
`impl` 토큰을 semantic definition request에 넣지 않았다. 또한 source `User` 토큰은 type declaration이
아니라 impl clause의 사용 위치라 declaration-range 비교만으로는 syntax site와 provider symbol을 맞출
수 없었다.

### 적용한 수정

1. impl clause의 source/target token에서 각각 LSP `textDocument/definition`을 호출한다.
2. 두 응답이 모두 현재 AnalysisPlan 안의 local type symbol 하나로 유일하게 해결될 때만 relationship을
   추가한다.
3. 두 사용 위치를 exact occurrence로 보존해 adapter가 declaration 위치와 다른 source token도 동일
   provider symbol임을 증명할 수 있게 했다.
4. name fallback이나 trait method를 type pair로 추론하는 경로는 추가하지 않았다.

### 검증 결과

fresh isolated cache에서 Rust canonical 결과는 implements 1 + overrides 1, explicit hierarchy site
matched 1/1, inventory failure 0이 됐다. implements evidence는 `impl Entity for User`의 `Entity` 토큰을
가리킨다.

### 재발 시 점검 순서

1. raw provider output에 type-level IMPLEMENTATION과 두 exact occurrence가 있는지 확인한다.
2. receipt의 explicit/matched site 수를 비교한다.
3. 같은 이름의 외부/로컬 trait를 둔 negative fixture에서 유일성 실패가 관계를 만들지 않는지 확인한다.
4. provider cache를 분리해 실행 코드 변경이 오래된 batch에 가려지지 않게 한다.

### 남은 한계 또는 후속 gate

대형 workspace 모드에서는 현재 type enrichment 제한이 적용된다. 수백~수천 파일 holdout에서 explicit
impl site를 전부 질의할지, provider batch API로 묶을지 성능·coverage 계약을 별도로 닫아야 한다.

---

## TS-2026-08-08-17 — 혼합 C/C++ header 문서 identity는 path 하나가 아니다

### 증상

10언어 semantic gate가 C fixture에서 `types.h` 중복 문서라고 실패했다. 실제 출력에는 같은 물리
header의 C semantic document와 C++ semantic document가 하나씩 있었다.

### 영향

정상적인 mixed C/C++ 저장소를 release failure로 막거나, 반대로 중복 검사를 없애 언어 안의 진짜
provider duplicate를 놓칠 수 있다.

### 근본 원인

gate가 `Group-Object path`를 사용했지만 provider 문서의 canonical identity는 compile context가 반영된
`(language, path)`다. 이 계약은 engine의 shared-header unit 분리와 이미 일치하고 있었다.

### 적용한 수정

semantic gate의 중복 검사를 `(language, path)`로 변경했다. 같은 언어·같은 path 중복은 계속 실패하고,
C와 C++가 각자 같은 header를 분석하는 경우만 허용한다.

### 검증 결과

수정 후 현재 release binary semantic gate는 TypeScript, JavaScript, Python, Java, C#, C, C++, Go,
Rust, Dart 10/10을 통과했다.

### 재발 시 점검 순서

1. duplicate key가 path만인지 `(language, path)`인지 확인한다.
2. shared header가 C와 C++ AnalysisUnit에 각각 소유됐는지 확인한다.
3. 같은 언어 안에서 동일 path 문서가 두 번 나온 경우는 계속 차단한다.

### 남은 한계 또는 후속 gate

동일 header가 서로 다른 compile flags/targets에서 다른 의미를 갖는 다중 C/C++ context는 향후
SemanticContext ID까지 document identity에 포함하는 canonical linker gate에서 검증해야 한다.

---

## TS-2026-08-08-18 — provider type occurrence를 모두 `uses_type`으로 만들면 지도 관계가 오염된다

### 증상

C/C++와 일부 LSP 언어에서 field/parameter/return뿐 아니라 local variable, constructor expression,
inheritance token, receiver/self까지 `uses_type`으로 나왔다.

### 영향

큰 프로젝트일수록 화면에 필요 없는 type edge가 폭증하고, 실제 구조 관계가 노이즈에 묻힌다.

### 근본 원인

provider가 말하는 “type occurrence”와 제품이 보여줄 “declaration-bound type relation”을 같은 것으로
취급했다. protocol flag만으로는 사용 위치의 수명을 구분할 수 없다.

### 적용한 수정

10언어 tree-sitter inventory에서 field, parameter, return, generic constraint의 exact target token만
수집한다. source definition과 target local type을 provider가 해결하고 exact range가 일치할 때만
`uses_type`을 만든다. hierarchy/self/local/expression은 fail-closed로 버린다.

### 검증 결과

10언어 관계 90건과 negative 22건을 고정한 독립 gate에서 FP/FN 0, exact evidence 100%, 2회 동일
digest를 통과했다.

### 재발 시 점검 순서

1. raw `TYPE_DEFINITION`/`USES_TYPE` 건수가 아니라 declaration inventory match를 본다.
2. relation evidence가 exact target type token인지 확인한다.
3. local body line과 hierarchy line에 `uses_type`이 생기지 않았는지 negative를 확인한다.

### 남은 한계 또는 후속 gate

실제 대규모 저장소에서 declaration-bound type edge 밀도와 UI relevance budget은 별도 holdout이 필요하다.

---

## TS-2026-08-08-19 — LSP selection range와 declaration range를 합치면 owner와 근거가 틀어진다

### 증상

Dart abstract method가 definition inventory에서 빠지고, method type annotation 관계가 바깥 class 소유로
붙었다.

### 근본 원인

definition occurrence의 `range`와 `enclosing_range`를 모두 전체 declaration 범위로 기록했다. exact
symbol name과 lexical owner를 구분할 수 없었다.

### 적용한 수정

definition `range`는 LSP selection/name span, `enclosing_range`는 전체 declaration로 분리했다. Dart의
`declaration` 아래 `function_signature`도 type owner 안에서는 Method로 inventory한다.

### 검증 결과

Dart fixture는 abstract Contract method, Base/Contract/Service type use, overrides를 모두 보존하며
extends 1, implements 2, mixes-in 1, overrides 2, uses-type 7을 통과했다.

### 재발 시 점검 순서

1. definition occurrence range가 이름 토큰만 가리키는지 본다.
2. enclosing range가 전체 method/class declaration인지 본다.
3. type relation source owner가 바깥 type으로 승격되지 않았는지 확인한다.

### 남은 한계 또는 후속 gate

provider별 multiline/annotation/decorator selection 차이는 실제 저장소 holdout에서 확대한다.

---

## TS-2026-08-08-20 — Python provider가 override API를 비워도 전역 이름 매칭을 쓰면 안 된다

### 증상

`Service(BaseService)`와 두 `execute` definition은 확인됐지만 overrides 관계가 0이었다.

### 근본 원인

Pyright가 해당 fixture에서 `textDocument/implementation` 결과를 주지 않았다. 이를 전체 저장소의 같은
메서드 이름으로 보완하면 동명이인 false positive가 된다.

### 적용한 수정

provider가 exact local `Service -> BaseService` inheritance pair를 증명하고, 양쪽 exact owner 아래에
local method definition이 있을 때만 같은 member를 연결한다. `__private` name-mangled member는 제외하고
repository-wide name fallback은 두지 않았다.

### 검증 결과

Python은 extends 1, overrides 1, declaration-bound uses-type 5를 독립 digest로 두 번 동일하게 통과했다.

### 재발 시 점검 순서

1. 먼저 local inheritance pair의 provider proof를 확인한다.
2. 양쪽 method의 parent symbol ID가 정확한 type인지 확인한다.
3. private name과 외부 base가 관계를 만들지 않는지 본다.

### 남은 한계 또는 후속 gate

multiple inheritance, protocol, decorator-replaced members는 별도 Python edge-case corpus가 필요하다.

---

## TS-2026-08-08-21 — 같은 C++ 템플릿 생성자가 두 label로 들어오면 definition FP가 된다

### 증상

clangd가 같은 15:13 생성자를 `BoxValue`와 `BoxValue<T>`로 내보내 definition gate가 117 TP,
1 FP가 됐다.

### 근본 원인

LSP symbol 중복 identity가 `(display name, selection 위치)`라서 같은 source definition의 provider 표시
차이를 서로 다른 정의로 취급했다.

### 적용한 수정

동일 document 안의 LSP definition identity를 `(selection line, selection character, symbol kind)`로
통일했다. lexical한 짧은 label과 더 넓은 declaration span, exact parent를 보존한다.

### 검증 결과

C++ extra provider definition은 1에서 0으로 줄었고 전체 definition 117/117, kind/owner 100%와 type
relation gate 90/90이 함께 통과했다.

### 재발 시 점검 순서

1. 같은 위치·kind의 document/workspace symbol 중복을 확인한다.
2. struct/type-alias처럼 같은 위치라도 kind가 다른 정상 정의는 합치지 않는다.
3. 선택한 label이 실제 lexical declaration 이름과 맞는지 확인한다.

### 남은 한계 또는 후속 gate

macro-generated definitions와 서로 다른 compile context의 같은 위치는 향후 SemanticContext ID까지 포함한
canonical linker에서 분리한다.

---

## TS-2026-08-08-22 — 개별 gate 통과와 release bundle 통과는 같은 조건이 아니다

### 증상

semantic 정답 gate는 단독 실행에서 35/35를 통과했지만, 상위 provider bundle gate 안에서는 정상 정답
항목의 `tokenOccurrence` 속성이 없다는 오류로 중단됐다.

### 근본 원인

선택 속성을 `$Expected.tokenOccurrence`로 직접 읽었다. 단독 실행에는 없던
`Set-StrictMode -Version Latest`를 상위 release script에서 상속하면, 속성 생략을 계약상 정상 기본값이
아니라 런타임 오류로 해석했다.

### 적용한 수정

`$Expected.PSObject.Properties['tokenOccurrence']`로 속성 존재를 먼저 확인하고, 없으면 첫 occurrence를
사용하도록 명시했다. StrictMode를 끄거나 정답 파일에 가짜 속성을 채우지 않았다.

### 검증 결과

같은 StrictMode 아래 semantic 35/35, import 39/39, type relation 90/90이 통과했다. 이어 signed catalog의
8개 provider pack을 임시 경로에 추출하는 전체 release bundle gate가 10/10 언어로 통과했다.

### 재발 시 점검 순서

1. 개별 gate뿐 아니라 실제 상위 release entry point에서 실행한다.
2. 선택 속성은 `PSObject.Properties`로 존재를 확인하는지 본다.
3. 실패를 숨기려고 StrictMode를 끄거나 fixture를 인위적으로 채우지 않는다.

### 남은 한계 또는 후속 gate

PowerShell gate의 선택 속성 접근을 공용 helper/schema validation으로 모으는 작업은 아직 남아 있다.

---

## TS-2026-08-08-23 — generic reference를 지우자 무관한 type digest가 바뀜

### 증상

지도에 필요 없는 `REFERENCES`를 공통 IR enum에서 제거했을 뿐인데 TypeScript type relation
`relationSetDigest`가 달라져 90-relation gate가 중단됐다. 관계 개수와 대표 positive/negative는 같았다.

### 근본 원인

type relation set key가 안정적인 관계 이름이 아니라 `relation_kind_rank` 숫자를 포함했다. rank를 compact
enum ordinal처럼 다시 매기면서, 삭제 항목 뒤의 `extends`/`implements`/`uses_type` 번호가 모두 당겨졌다.
즉 사실이 아니라 내부 정렬 구현이 품질 digest를 바꿨다.

### 적용한 수정

relation rank를 append-only wire-order ID로 정의하고 삭제된 generic reference의 rank 5를 영구 예약했다.
정답표의 digest를 새 값으로 덮거나 type relation을 다시 학습시키지 않았다.

### 검증 결과

기존 per-language relation-set digest가 그대로 복구됐고, 10언어 관계 90/90·negative 22건·2회 결정성·
direct/donor parity가 통과했다. generic reference는 0개 영속되며 호출 35/35와 import/export 39/39도
변하지 않았다.

### 재발 시 점검 순서

1. set digest가 enum ordinal, 배열 index, map iteration order를 포함하는지 확인한다.
2. vocabulary 삭제 시 살아 있는 wire ID를 재번호화하지 않는다.
3. digest mismatch를 발견하면 정답 hash부터 갱신하지 말고 full audited set의 의미 변화를 먼저 대조한다.

### 남은 한계 또는 후속 gate

향후 canonical edge vocabulary도 stable string/append-only wire ID 규칙을 schema validation으로 고정해야 한다.

---

## TS-2026-08-08-24 — 같은 Language IR을 두 번 만들어 parity를 보면 정확도도 성능도 착시다

### 증상

언어별 source 하나를 1.1MB로 키운 gate에서 provider 실행은 106초였지만 Language IR 변환이
507초를 사용했다. 작은 fixture에서는 드러나지 않았고 정확도 gate는 모두 통과했다.

### 근본 원인

스케줄러의 `ProviderUnitBatch`를 direct IR로 한 번 변환한 뒤, 같은 batch를 legacy output으로 합치고 그
output을 다시 donor IR로 변환했다. 두 경로는 동일 adapter를 공유했으므로 exact parity는 독립 구현의
교차 검증이 아니라 사실상 같은 계산의 자기 비교였다.

### 적용한 수정

`ProviderUnitBatch`를 유일한 stream authority로 정했다. 한 번의 deterministic merge에서 job-scoped
JSONL Language IR과 임시 `language-index.v2` compatibility projection을 함께 만들며, donor→IR 재변환과
provider-parity receipt를 제거했다. JSONL은 임시 파일에 쓴 뒤 flush/sync/atomic rename하고 incomplete
artifact는 publish하지 않는다.

gate 공용 helper는 authority schema, snapshot ID, stream-set digest, record count, byte count, complete,
content SHA-256을 migration receipt와 대조한다. 두 번 실행에서 실제 JSONL byte digest도 같아야 한다.

### 검증 결과

Code Memory 261/261, definition 117/117, CALLS/CONSTRUCTS 35/35, import 39/39, type relation 90/90과
signed provider 8-pack/10-language release gate가 통과했다. unit test는 artifact의 모든 JSONL record를
역직렬화하고 파일 byte SHA-256을 직접 다시 계산한다.

### 재발 시 점검 순서

1. provider batch를 IR로 바꾸는 호출이 실행 경로에 하나뿐인지 확인한다.
2. compatibility output을 다시 IR input으로 쓰는 코드가 생기지 않았는지 본다.
3. 같은 함수의 두 결과를 parity라고 부르지 말고 실제 artifact의 content digest와 closed record count를 본다.

### 남은 한계 또는 후속 gate

JSONL은 canonical linker를 위한 job staging artifact다. 최종 product DB용 SQLite Fact Import Bundle과
Tauri atomic publish는 Batch D/F에서 별도로 구현해야 한다.

---

## TS-2026-08-08-25 — capability마다 같은 source를 parsing하면 대형 파일에서 선형 배수가 된다

### 증상

donor 재변환을 제거한 뒤에도 1.1MB Python 파일 하나의 `direct_language_ir_stream_emission`이
22~23초였다. 다른 언어의 같은 크기 파일은 대부분 0.1초대였다.

### 근본 원인

definition, import, type hierarchy, type use inventory가 각각 tree-sitter parser를 실행했고 type use는
내부에서 definition과 hierarchy를 다시 호출했다. 한 file당 최대 다섯 번 같은 syntax tree를 만들었다.
SCIP/LSP provider enrichment도 type relation과 type use를 따로 parsing했다. Python의 큰 comment-heavy
tree를 반복 생성하면서 capability 수만큼 비용이 곱해졌다.

### 적용한 수정

Language IR adapter는 source마다 `parse_tree`를 한 번만 호출하고 definition/import/type relation/type use
inventory에 같은 root를 전달한다. type use는 이미 계산한 definition과 hierarchy 결과를 입력으로 받는다.
SCIP/LSP provider는 `SyntaxTypeInventory` 한 번으로 relation과 use를 함께 얻고 같은 파일 결과를 재사용한다.
개별 parse wrapper는 test build에만 남겨 production code가 다시 호출하지 못하게 했다.

### 검증 결과

동일한 10언어, 언어별 1.1MB source, 2회 결정성 gate에서 다음처럼 줄었다.

| 구간 | 수정 전 | 수정 후 | 감소 |
| --- | ---: | ---: | ---: |
| semantic phase direct IR 합계 | 46,198ms | 8,280ms | 82.1% |
| definition phase direct IR 합계 | 47,050ms | 8,283ms | 82.4% |
| 최악 단일 Python direct IR | 23,015ms | 3,708ms | 83.9% |

definition 117/117, CALLS/CONSTRUCTS 35/35, content digest 2회 결정성과 원본 불변은 그대로 통과했다.

### 재발 시 점검 순서

1. 한 source file의 `parse_tree` 호출 수를 capability 수가 아니라 1로 본다.
2. type use 계산이 definition/hierarchy wrapper를 다시 호출하지 않는지 확인한다.
3. 정확도 digest가 바뀌면 최적화 성공으로 간주하지 말고 inventory 의미 변화부터 조사한다.
4. 총 실행시간만 보지 말고 provider와 `direct_language_ir_stream_emission` timing을 분리한다.

### 남은 한계 또는 후속 gate

3.7초는 1.1MB Python의 cold full syntax parse를 포함한다. 실제 수백~수천 파일 workspace의 peak memory,
parallel admission, cancellation, unchanged-file inventory cache는 별도 holdout과 incremental Batch H에서 닫는다.

---

## TS-2026-08-08-26 — raw provider metadata coverage를 제품 definition 정확도로 쓰지 않는다

### 증상

definition name/kind/owner는 117/117을 통과했지만 compatibility raw provider symbol 162개 중
signature가 있는 것은 60개뿐이었고, 기존 Language IR visibility는 117개 모두 `unknown`이었다.
언어별 provider가 주는 부가 필드 차이를 그대로 제품 품질로 보면 이미 정확한 definition까지 낮은
신뢰도로 오판하거나, 반대로 raw symbol 수를 억지로 채우기 위해 제품에 필요 없는 데이터를 수집하게 된다.

### 영향

지도에서 함수 형태와 public/internal 경계를 설명할 수 없고, provider가 바뀔 때 같은 source의 metadata가
달라질 수 있었다. raw 60/162를 제품 정확도처럼 표시하면 실제 사용자가 보는 definition 분모와도 맞지 않는다.

### 잘못 짚기 쉬운 원인

provider 성능이나 언어 자체의 한계가 아니었다. 여러 provider의 raw symbol schema가 서로 다른 것과,
최종 지도에서 제외할 symbol까지 같은 분모에 섞은 측정 계약이 문제였다.

### 근본 원인

name/kind/owner는 source syntax로 감사하면서 signature/visibility만 provider 선택 필드에 맡겼다.
즉 하나의 definition 안에서 권위가 둘로 갈렸고, 제품이 실제로 필요한 callable declaration과 public surface의
독립 정답표가 없었다.

### 적용한 수정

동일 source declaration을 최소 metadata의 권위로 정했다. callable signature는 decorator, body,
constructor initializer를 제외한 declaration header만 보존하고, visibility는 명시 modifier와 언어별
정적 기본 규칙으로 계산한다. documentation, annotation, local variable, statement, body 요약은 추가하지
않았다. migration receipt를 v5로 올려 언어별 metadata count, digest, audit sample을 기록하고,
`definitions.v1.json`에 10언어 사람 검토 metadata 사례 37개를 고정했다.

C++ 생성자 initializer가 signature에 섞이는 결함과 Rust public trait 구현 member가 private로 내려가는
결함은 각각 source 문법 의미에 맞게 고치고 회귀 사례로 남겼다. 입력 인자가 늘어 clippy가 지적한 helper는
경고 suppression 대신 `DefinitionMetadataInput` context object로 묶어 책임 경계를 명확히 했다.

### 검증 결과

- definition name/kind: 117/117, FP/FN 0
- owner: 55/55
- callable declaration signature: 63/63
- known visibility: 117/117
- reviewed metadata case: 37/37
- 10/10 언어, 2회 metadata/content digest 결정성
- Code Memory 263/263, fact-model 15/15, Tauri 38/38
- fmt, locked check, clippy `-D warnings`, locked release build 통과
- signed provider 8-pack을 임시 위치에 추출한 release gate에서 네 독립 정답 gate 재통과

### 재발 시 점검 순서

1. raw provider symbol count와 최종 reviewed definition count를 같은 분모로 쓰지 않는다.
2. signature가 source declaration header만 포함하고 body·initializer·decorator를 제외하는지 본다.
3. visibility 기본값이 언어의 실제 namespace/type/member 규칙을 따르는지 본다.
4. 정답 수를 갱신하기 전에 새 결과가 사람 검토 metadata 사례와 digest를 모두 통과하는지 확인한다.
5. metadata를 늘릴 때 최종 시각화 소비자가 없는 필드는 hard cut한다.

### 남은 한계 또는 후속 gate

이 gate는 고정된 10언어 corpus의 최소 definition metadata를 인증한다. 임의 저장소 전체 정확도 100%를
뜻하지 않는다. 실제 compiler/provider execution context와 frozen·OSS·대규모 holdout은 별도 gate이며,
definition relevance는 provider가 추측하지 않고 canonical linker가 source flags·ownership·typed relation으로
결정해야 한다.

---

## TS-2026-08-08-27 — 실행 context를 계획값으로 추정하거나 positive만 검사하지 않는다

### 증상

10개 언어 provider는 모두 실행되고 core fact gate도 통과했지만, 실행 조건 영수증은 exact 0/10,
partial 10/10이었다. 실제 context 구현 뒤 단독 gate는 통과했으나 서명된 provider 8-pack 전체 release
gate에서는 Java가 `not_executed`가 됐고, type-relation gate의 Java/C/C++/Go/Rust/Dart set digest도
이전 정답과 달라졌다.

### 영향

같은 source라도 target, build tag, feature, source set, language version이 다른 실행을 같은 snapshot으로
오인할 수 있었다. 반대로 provider 실행이 없는 상태를 Language IR 자체가 거부하면 syntax와 project model로
정확히 아는 사실까지 잃는다. release 설치 경로에서만 발생하는 실패를 로컬 provider 통과로 숨길 위험도
있었다.

### 잘못 짚기 쉬운 원인

Java 분석기나 타입 관계 정확도가 무너진 것이 아니었다. release 실패의 실제 진단은 Windows
`os error 267`이었고, 이전/현재 raw 관계를 항목별 비교한 결과 제품 관계 목록은 동일했다. type digest
차이는 실행-context를 반영해 Analysis Unit identity가 바뀐 데서 왔다. Dart raw provider의 SDK `Object`
심볼은 provider 설치 절대 경로가 달랐지만, 제품에 남기는 13개 local type relation에는 들어오지 않았다.

### 근본 원인

1. 실제 provider가 사용한 config/환경이 아니라 계획 단계의 context를 결과 identity와 분리해 기록했다.
2. 설정이 있는 positive fixture만 보면 누락 context를 exact로 승격하는 회귀를 잡을 수 없었다.
3. `not_executed`를 “어떤 semantic fact도 없음”으로 해석해 provider provenance와 syntax/project-model
   provenance를 구분하지 못했다.
4. context gate가 report UUID 아래에 cache UUID를 또 중첩했다. 엔진이 app cache 하위 경로를 더 붙이자
   JDTLS process 경로가 Windows 한계를 넘어 시작하지 못했다.
5. type relation set digest는 Analysis Unit ID를 포함하므로 identity 계약 변경 뒤 사람 검토 없이 이전
   digest만 유지할 수 없다.

### 적용한 수정

- Language IR v2 header에 실제 `ProviderExecutionContext`를 넣고 snapshot/context digest에 포함했다.
- migration receipt v6와 reconciliation receipt v3가 같은 context-set digest를 가져야 통과시켰다.
- 10개 언어별 mode/dimension/config artifact path·usage·SHA-256를 실제 runner 입력에서 수집했다.
- 독립 정답을 정상 9 projects/10 languages와 config 제거 9 variants로 나누고 각각 두 번 실행한다.
- `not_executed`에서도 syntax/project-model evidence는 허용하되 SCIP/LSP/compiler evidence와
  provider/compiler resolution은 fail-closed로 금지했다.
- gate cache는 짧고 격리된 OS temp 경로를 사용하고, 정규화한 temp 자식 경로임을 확인한 뒤 정리한다.
  mode 불일치 시 index status와 provider diagnostics를 함께 출력한다.
- identity 변경으로 달라진 type digest는 이전/현재 raw relation과 90개 전체 audit를 대조한 뒤 새 값으로
  동결했다. relation count/kind/endpoint/evidence 정답을 약화하지 않았다.

### 검증 결과

- configured context: 9 projects, 10 languages, exact 10/10, 2회 결정성
- missing context: 9 variants, false exact 0, 언어별 partial/not-executed와 typed missing 고정
- config artifact SHA-256 및 context/snapshot/stream/content digest 대조 통과
- type relation: 10/10 languages, 90/90, negative 22, 2회 결정성
- Code Memory 275/275, fact-model 16/16, clippy `-D warnings`, release build 통과
- signed provider 8-pack release chain에 context gate를 포함

### 재발 시 점검 순서

1. provider 실행 성공과 context exact를 같은 값으로 보지 않는다.
2. positive와 config-removed variant를 반드시 함께 실행한다.
3. `not_executed` fact가 어떤 provenance를 주장하는지 먼저 본다.
4. 로컬 provider뿐 아니라 임시 위치에 새로 푼 signed bundle로 실행한다.
5. process 시작 오류는 message를 버리지 말고 status/diagnostic과 실제 cache path 길이를 확인한다.
6. identity schema 변경 뒤 digest가 달라지면 count만 갱신하지 말고 이전/현재 관계를 전수 대조한다.

### 남은 한계 또는 후속 gate

이 matrix는 고정된 소형 project의 정상/설정 누락 경계를 인증한다. 실제 monorepo의 여러 target/TU,
Gradle/Maven reactor, multi-target .NET, Go build constraints, Rust feature 조합, Dart workspace와 수백~수천
파일의 resource/cancel 조건은 frozen holdout으로 추가 인증해야 한다. 다음 구현 경계는 canonical
normalizer/linker이며 AI가 이 context나 endpoint를 보완해서는 안 된다.

---

## TS-2026-08-08-28 — Language IR과 canonical bundle이 서로 다른 snapshot 공식을 쓰지 않는다

### 증상

Language IR v2는 실제 Analysis Plan과 provider execution-context fingerprint를 snapshot에 포함했지만,
초기 canonical manifest validator는 예전 workspace/source/config/provider 조합으로 snapshot을 다시
계산했다. 같은 실행을 가리키는 두 산출물이 서로 다른 정체성을 주장할 수 있었다.

### 영향

새 context가 반영된 정상 IR을 canonical 단계가 거부하거나, 반대로 다른 실행 조건의 사실을 같은
snapshot으로 받아들일 수 있다. 캐시 재사용·증분 비교·대화 stale 판정이 모두 잘못된다.

### 잘못 짚기 쉬운 원인

digest 문자열이나 JSON 직렬화 순서 문제가 아니었다. snapshot을 만드는 입력 목록 자체가 두 군데에서
갈라진 것이 원인이었다.

### 근본 원인

Language IR 전환 중 새 identity 공식을 adapter 안에서 직접 조합했고, shared contract의 manifest validator는
구 공식을 유지했다. 하나의 개념에 두 계산 구현이 존재했다.

### 적용한 수정

shared fact-model의 `SnapshotId::from_execution_inputs`를 단일 권위로 만들고 workspace, source manifest,
analysis plan, provider set, execution-context set을 고정 순서로 받게 했다. Language IR adapter,
canonical linker, `FactBundleManifest::validate`가 모두 같은 helper를 호출한다. canonical 단계는 IR artifact의
content SHA-256과 record count도 다시 검사한다.

### 검증 결과

- fact-model 16/16, Code Memory 280/280
- 10언어 configured/missing-context 2회 snapshot/content digest 검증
- canonical receipt/manifest/artifact snapshot exact match
- signed provider release chain 통과

### 재발 시 점검 순서

1. snapshot 생성 코드가 shared helper 밖에 새로 생겼는지 검색한다.
2. plan/provider/context 중 하나를 바꾼 테스트에서 snapshot이 반드시 달라지는지 본다.
3. IR, canonical receipt, manifest, artifact의 snapshot을 한 gate에서 대조한다.
4. digest가 달라졌다고 정답을 먼저 갱신하지 말고 입력 domain 변경 여부를 확인한다.

### 남은 한계 또는 후속 gate

DB catalog를 Code+DB bundle에 넣을 때 DB snapshot/config digest를 기존 language snapshot에 억지로 끼우지
말고, 명시적인 composite generation identity 계약을 추가해야 한다.

---

## TS-2026-08-08-29 — semantic identity, byte integrity, 회계 분모를 섞지 않는다

### 증상

초기 canonical 구현에는 세 문제가 동시에 있었다.

1. provider identity 수에서 retained canonical node 수를 빼 pruning 수를 계산했다.
2. evidence summary와 diagnostic/gap message가 바뀌면 semantic digest도 바뀌었다.
3. 파일명이 snapshot+semantic digest뿐이라, 같은 의미지만 사람용 문구가 다른 SQLite payload가 같은
   경로에서 충돌할 수 있었다.

### 영향

partial declaration/alias가 하나의 canonical node로 합쳐지는 정상 동작을 제거로 과장하고, 번역·문구 수정이
지도를 새 snapshot처럼 흔들며, 실제 byte가 다른 immutable artifact를 같은 이름으로 게시하려 할 수 있었다.

### 잘못 짚기 쉬운 원인

SQLite 비결정성이나 AI 변동 문제가 아니었다. 서로 다른 목적의 identity와 서로 다른 단위의 수를 같은
필드·공식으로 사용한 계약 오류였다.

### 근본 원인

- provider-native identity와 canonical node를 같은 분모로 취급했다.
- 사람이 읽는 설명과 기계 의미를 같은 JSON 전체 hash로 묶었다.
- semantic digest를 payload content address처럼 사용했다.

### 적용한 수정

- receipt를 provider definition identity, canonical definition node, retained node, pruned node 네 수치로
  분리하고 pruning은 canonical node끼리만 계산한다.
- evidence는 summary를, gap/issue는 message/remediation을 제외한 typed projection으로 semantic digest를
  계산한다. 중복 gap/issue도 typed semantic key로 결정적으로 병합한다.
- full SQLite bytes는 별도 bundle SHA-256로 보호하고 `canonical-<bundleDigest>`를 immutable filename으로
  사용한다. 외부 manifest가 마지막 complete marker다.
- release gate가 실제 bundle bytes를 다시 hash하고 receipt/manifest/artifact identity와 회계를 대조한다.

### 검증 결과

- 같은 diagnostic 의미·다른 문구: semantic digest 동일, bundle digest와 path는 다름
- 동일 입력 10언어 2회: semantic digest와 SQLite byte digest 모두 동일
- 언어별 1.1MB source에서도 동일 결정성
- dangling endpoint 0, evidence 없는 confirmed 0, duplicate logical edge 0
- Code Memory 280/280, clippy `-D warnings`, release build 통과

### 재발 시 점검 순서

1. 수치의 분자가 provider identity인지 canonical node인지 먼저 적는다.
2. 사람이 고쳐 쓸 수 있는 문구가 stable ID/semantic digest 입력에 들어가는지 본다.
3. semantic digest와 byte payload digest를 같은 용도로 쓰지 않는다.
4. manifest를 먼저 게시하지 말고 payload close/fsync/hash/rename 뒤 마지막에 게시한다.
5. 동일 의미·다른 운영 문구, 동일 입력 2회, alias merge 세 회귀를 함께 실행한다.

### 남은 한계 또는 후속 gate

immutable bundle retention/current+previous generation GC와 crash injection은 Tauri publish 단계에서 별도
검증해야 한다. 지금은 미완성 `.tmp`가 제품 truth가 되지 않는 것까지 보장한다.

---

## TS-2026-08-08-30 — 선택적 capability parser 실패로 언어 전체를 버리지 않는다

### 증상

Dart Shelf fixture에서 LSP는 definition, call, import, framework registration을 정상 반환했지만, 보조
Tree-sitter Dart parser가 일부 type syntax를 incomplete parse로 판정해 언어 unit 전체가 실패했다.

### 근본 원인

provider가 확인한 핵심 사실과 선택적 source type enrichment가 하나의 `Result` 실패 경로를 공유했다.
서로 독립인 capability가 같은 실패 반경을 가졌다.

### 적용한 수정

LSP와 SCIP document 처리에서 선택적 type syntax 분석 실패를 `TypeRelations` partial coverage로 제한했다.
이미 확인한 definition/call/import/framework fact와 provider provenance는 유지한다. 단, 실패한 capability의
결과를 다른 근거로 추측해 채우지는 않는다.

### 검증 결과

Dart Shelf flow가 복구됐고 framework 전체 10/10, Code Memory 283/283, 10언어 definition/call/import/type
독립 gate가 통과했다.

### 재발 시 점검 순서

1. 실패한 parser가 필수 provider인지 선택적 enrichment인지 구분한다.
2. 해당 실패가 어떤 capability receipt와 gap만 낮춰야 하는지 확인한다.
3. 이미 source/provider로 확인한 다른 capability record가 사라지지 않는지 비교한다.

### 남은 한계 또는 후속 gate

실제 저장소 holdout에서 언어별 선택 capability 실패를 주입해 동일한 격리를 추가 검증한다.

---

## TS-2026-08-08-31 — framework 규칙 bytes와 adapter version을 snapshot 밖에 두지 않는다

### 증상

source와 provider binary가 같으면 framework pack 규칙을 바꿔도 같은 snapshot identity를 재사용할 수
있었다. route method/path 또는 handler 판정이 달라졌는데 이전 canonical artifact가 유효해 보일 위험이었다.

### 근본 원인

분석 결과를 결정하는 pack JSON과 adapter semantic version이 analyzer identity 입력이 아니었다.

### 적용한 수정

선택된 framework pack bytes와 typed Framework IR adapter version으로 analyzer-set digest를 만들고,
executed provider-set을 통해 canonical snapshot identity에 포함했다.

### 검증 결과

동일 bytes는 같은 digest, pack JSON 한 항목 변경은 다른 digest를 만드는 단위 테스트와 283개 전체 회귀를
통과했다.

### 재발 시 점검 순서

1. 결과 의미를 바꾸는 규칙 파일이 analyzer-set digest에 들어가는지 본다.
2. code-only semantic 변경이면 adapter version을 올린다.
3. snapshot이 달라지기 전에 golden digest만 갱신하지 않는다.

### 남은 한계 또는 후속 gate

ORM/test/event typed adapter도 같은 규칙으로 analyzer bytes/version을 identity에 포함해야 한다.

---

## TS-2026-08-08-32 — 한 capability의 완료 상태를 두 adapter가 동시에 소유하지 않는다

### 증상

language adapter는 `FrameworkBindings`를 unsupported로 기록하면서 framework adapter는 실제 route와
handler를 생성할 수 있었다. 한 unit에 서로 반대인 완료 영수증이 생길 수 있었다.

### 근본 원인

capability ownership과 데이터 생산자 책임이 분리되지 않았다. 또한 raw duplicate donor 수를 coverage
분모로 쓰면 같은 사실이 반복될수록 품질이 낮아지는 회계 오류가 있었다.

### 적용한 수정

`FrameworkBindings` receipt는 canonical framework adapter만 생성한다. raw donor 후보 수와 exact duplicate
제거 뒤 planned route 수를 분리하고 `planned = emitted + rejected`를 강제한다.

### 검증 결과

raw 후보 4개가 valid duplicate와 rejected duplicate를 거쳐 planned 2, emitted 1, rejected 1로 계산되고,
unit receipt가 하나만 생성되는 회귀 테스트가 통과했다.

### 재발 시 점검 순서

1. capability마다 최종 receipt producer가 하나인지 찾는다.
2. raw signal 수, deduplicated plan 수, emitted/rejected 수를 별도 분모로 둔다.
3. complete/partial이 실제 fact·relation 수와 모순되지 않는지 검증한다.

### 남은 한계 또는 후속 gate

새 ORM/DB/test/event capability를 추가할 때 같은 single-owner matrix를 먼저 계약한다.

---

## TS-2026-08-08-33 — 표시 이름을 기계 데이터로 다시 parsing하지 않는다

### 증상

`GET /orders` 같은 route label만 저장하면 소비자가 공백을 나누어 method/path를 복구해야 한다. 표시 문구,
번역, formatting이 바뀌면 identity와 query가 깨질 수 있었다.

### 근본 원인

사람용 label과 기계가 써야 할 typed 속성을 같은 문자열로 취급했다.

### 적용한 수정

shared fact-model에 `FactNodeDetails::HttpRoute { method, path }`를 추가했다. method 대문자, 절대 path,
`{METHOD} {path}` qualified identity 일치를 validation으로 강제하고 details 누락은 fail-closed한다.

### 검증 결과

정상 route는 통과하고 details 누락, 소문자 method, identity 불일치는 모두 거부하는 fact-model 계약 테스트가
추가돼 17/17을 통과했다.

### 재발 시 점검 순서

1. UI label을 split/regex로 분석하는 소비자가 생겼는지 찾는다.
2. 정렬·필터·연결에 필요한 값은 typed details/edge인지 확인한다.
3. 표시 문구는 semantic identity 입력에서 분리한다.

### 남은 한계 또는 후속 gate

DB table/column, queue topic, external endpoint도 표시 이름이 아니라 typed details를 가져야 한다.

---

## TS-2026-08-08-34 — framework detection, route fact, handler binding은 서로 다른 주장이다

### 증상

의존성이나 `app.get` 문자열을 발견했다는 이유만으로 route와 `Handles`를 함께 만들면 dynamic path,
comment/example, unresolved callback에서 없는 연결을 만들 수 있었다.

### 근본 원인

analyzer 선택 신호, source-backed endpoint, exact symbol relation의 증거 수준을 한 단계로 취급했다.

### 적용한 수정

1) pack signal은 analyzer 선택만 한다. 2) 정적 method/path와 exact source evidence가 있어야 Framework IR
route를 만든다. 3) 기존 provider symbol identity가 정확히 해결될 때만 `Handles`를 만든다. handler가 없으면
route는 유지하고 typed gap을 남긴다.

### 검증 결과

9개 HTTP 언어에서 typed route + `Exposes` + `Handles`를 canonical bundle까지 확인했고, evidence 없는
candidate와 동적/잘못된 route는 게시되지 않는다. 전체 flow gate는 C event donor를 포함해 10/10이다.

### 재발 시 점검 순서

1. detection만으로 endpoint가 생기는 경로가 있는지 확인한다.
2. route evidence와 handler evidence를 따로 검사한다.
3. handler 실패 시 route까지 사라지는지, 반대로 guessed edge가 생기는지 둘 다 본다.

### 남은 한계 또는 후속 gate

middleware/RPC/GraphQL/frontend route는 각각 독립 typed 계약과 positive/negative fixture가 필요하다.

---

## TS-2026-08-08-35 — 정답 fixture의 잘못된 project config를 analyzer 결함으로 오인하지 않는다

### 증상

Java Spring flow에서 route는 잡혔지만 handler→service call이 없었다. JDT 분석 정확도 문제처럼 보였으나
fixture의 Maven dependency에 version이 없어 project model이 유효하지 않았다.

### 근본 원인

source 정답만 검토하고, provider가 실제 읽는 build/config까지 유효한 정답 입력으로 검증하지 않았다.

### 적용한 수정

Spring Web dependency version 6.1.8을 명시해 유효한 POM으로 만들었다. 게이트는 provider 실행 상태와
diagnostic을 함께 출력하므로 source mismatch와 project configuration 실패를 분리한다.

### 검증 결과

Java Spring MVC route→handler→service가 복구됐고 framework flow 10/10이 통과했다.

### 재발 시 점검 순서

1. provider status와 diagnostic을 먼저 확인한다.
2. fixture manifest/build file이 실제 도구로 유효한지 본다.
3. 잘못된 fixture에 맞추려고 analyzer 정확도 규칙을 완화하지 않는다.

### 남은 한계 또는 후속 gate

frozen OSS holdout에서는 lockfile/toolchain 없이도 재현 가능한 fixture packaging 정책을 별도로 둔다.

---

## TS-2026-08-08-36 — 상위 TypeScript config의 위치를 provider 실행 root로 기록하지 않는다

### 증상

`D:\meeting-overlay-assistant`의 7개 AnalysisPlan unit과 1,037개 언어-파일을 provider가 모두 처리한 뒤,
결과 게시 직전에 아래 오류로 전체 분석이 중단됐다.

`typescript provider executed root . but AnalysisPlan unit ... requires legacy/frontend`

`legacy/frontend`는 자체 `package.json`으로 분리된 계획 단위지만 TypeScript project model은 저장소 root의
`tsconfig.json`을 상속했다. 약 9분 이상 provider를 실행하고도 language/architecture/canonical 결과를 하나도
받지 못했다.

### 영향

실제 source나 관계가 잘못된 것이 아니라 실행 영수증 한 필드가 틀린 상태였지만, 정확성 gate가 전체 snapshot
게시를 올바르게 차단했다. 이 gate를 완화하면 이후 cache·증거·coverage가 어느 분석 단위에서 만들어졌는지
거짓으로 기록될 수 있다.

### 잘못 짚기 쉬운 원인

AnalysisPlan이 nested package를 잘못 나눴거나 `actual_root != planned_root` 검사가 과도한 것이 아니었다.
계획 root, provider process의 cwd/`--cwd`, 참고한 config 파일의 소유 폴더는 서로 다른 값일 수 있다.

### 근본 원인

SCIP TypeScript runner는 process를 계획된 `legacy/frontend`에서 실행하고 provider `--cwd`도 같은 값으로
전달했다. 그런데 execution-context 영수증을 만들 때 explicit/generated config의 `parent()`를
`analysis_root`로 덮어썼다. 상위 `tsconfig.json`은 실행에 사용한 **증거/config**인데 이를 실행 **경계**로
오인한 것이다.

### 적용한 수정

- `scip_execution_context`가 scheduler에서 받은 계획 root를 유일한 execution root로 기록한다.
- 상위 config는 `ExplicitArgument` 또는 `GeneratedLineage` artifact와 SHA-256으로 계속 보존한다.
- root/config/dimension 불일치 reconciliation은 완화하지 않았다.
- 새 executable hash가 provider cache key에 들어가므로 잘못된 구형 execution-context cache는 재사용되지 않는다.

### 검증 결과

- nested `legacy/frontend` + root `tsconfig.json` 회귀에서 실제 root는 `legacy/frontend`, config artifact는
  `tsconfig.json`으로 동시에 보존됐다.
- Code Memory **286/286**, clippy `-D warnings` 통과.
- 실제 `D:\meeting-overlay-assistant` 재분석: AnalysisPlan unit 7, scheduled 언어-파일 1,037,
  execution-context reconciliation 7ms, provider root mismatch 0.
- 최종 `language-index.json` 19,577,485 bytes, `architecture-index.json` 5,081,492 bytes와 immutable canonical
  SQLite 44,744,704 bytes가 생성됐다.
- canonical manifest: node 5,276, edge 12,011, evidence 13,112, file coverage 1,037, dangling/root 차단 오류 0.

### 재발 시 점검 순서

1. AnalysisPlan root, 실제 process cwd/workspace root, config artifact path를 세 열로 분리한다.
2. parent config를 참고했다는 이유로 package 실행 root가 위로 승격되는지 본다.
3. mismatch gate를 지우기 전에 runner가 어떤 값을 영수증에 넣었는지 확인한다.
4. root config + nested package가 함께 있는 실제 monorepo topology를 release 전 실행한다.

### 남은 한계 또는 후속 gate

scip-typescript가 상위 config를 해석하며 다른 shard 파일을 함께 살펴보는 로그는 남을 수 있다. canonical
payload는 schedule의 exact file scope로 다시 제한된다. per-shard exact generated config는 중복 parsing과
provider noise를 줄이는 성능 hardening으로 별도 측정한 뒤 적용한다.

---

## TS-2026-08-08-37 — 엔진 stderr 전체를 사용자 오류 메시지로 렌더링하지 않는다

### 증상

분석 실패 시 수백 줄의 `@codebase-workspace-progress` JSON, scheduler timing, provider 로그와 마지막 실제
오류가 하나의 빨간 toast에 들어가 앱 전체 화면을 덮었다. 사용자는 실패 원인도, 재시도 위치도 읽을 수
없었다.

### 영향

진단 정보가 많을수록 제품 UI가 더 망가지는 역전이 생겼다. 긴 한 줄은 flex toast 폭을 viewport 밖으로
늘려 지도와 inspector까지 가렸다.

### 근본 원인

Tauri가 실패한 `EngineRunResult.stderr.trim()` 전체를 command error detail로 반환했고, frontend
InfoBar에는 최대 폭·줄바꿈·높이 제한이 없었다. progress observer와 사용자 오류 경계가 분리되지 않았다.

### 적용한 수정

- `code-memory-language:`가 붙은 마지막 actionable error를 우선 선택한다.
- progress marker, scheduler/cache/timing telemetry와 provider log는 사용자 detail 후보에서 제외한다.
- fallback detail도 Unicode 문자 기준 800자로 제한한다.
- Fluent error InfoBar를 viewport 최대 720px로 제한하고 `overflow-wrap:anywhere`, 제한 높이 scroll을 적용했다.
- 원본 stderr capture/redaction과 개발 진단 경로는 유지한다.

### 검증 결과

진행 로그 + timing + 실제 root 오류 fixture에서 실제 오류 한 줄만 남고, progress-only stderr는 일반 실패
문구로 내려가며, 2,000자 Unicode 오류는 800자 + 말줄임으로 제한된다. Tauri 62 passed/3 ignored,
frontend 6/6, typecheck, lint, production frontend build가 통과했다.

### 재발 시 점검 순서

1. machine progress/receipt와 user-facing error가 같은 문자열 경로를 쓰는지 확인한다.
2. 마지막 줄만 무조건 쓰지 말고 명시적 engine error prefix를 먼저 찾는다.
3. backend 길이 제한과 frontend layout containment를 둘 다 검증한다.
4. 실제 실패 fixture에 수백 개 progress heartbeat와 긴 unbroken token을 포함한다.

### 남은 한계 또는 후속 gate

오류 코드별 한국어 remediation/action button은 별도 typed command-error 계약에서 확장한다. 이번 수정은
원인을 숨기지 않으면서 화면 붕괴를 막는 공통 경계다.

---

## TS-2026-08-08-38 — 검증 산출물을 source tree와 시스템 Temp에 무기한 쌓지 않는다

### 증상

반복한 provider gate, ground-truth audit, semantic compile 검증이 고유 이름의 폴더를 계속 만들었다.
시스템 Temp에는 프로젝트 prefix를 가진 폴더가 631개·약 12.1GiB, 저장소 안에는 `.build` 1.61GiB와
`code_memory/build` 976MiB를 포함한 무시된 산출물이 약 2.67GiB 남아 있었다. Git에는 보이지 않지만
탐색기 구조와 디스크가 계속 더러워졌다.

### 영향

source/fixture와 일회성 output의 경계가 흐려지고, 오래된 JSON·SQLite·provider cache를 현재 검증 결과로
오인할 수 있다. 저장 공간 문제보다 더 큰 위험은 이전 실행 산출물을 새 실행의 증거처럼 읽는 것이다.

### 근본 원인

수동 검증 명령과 일부 gate가 고유 output/cache 경로를 만들었지만 성공·실패 뒤 수명 정책이 없었다.
Rust의 정상 `target`, 설치 provider, 앱 workspace store와 일회성 debug output도 정리 관점에서 구분되지
않았다.

### 적용한 수정

- `scripts/cleanup-development-artifacts.ps1`을 추가했다. 기본은 preview이며 시스템 Temp의 프로젝트 전용
  artifact 중 300MiB 이상만 고른다.
- 저장소 산출물은 `-IncludeRepositoryOutputs`, 분석 cache는 `-IncludeAnalysisCache`, 실제 삭제는
  `-Execute`를 각각 명시해야 한다.
- 삭제 허용 root와 정확한 prefix/allowlist를 먼저 절대경로로 검증한다.
- source, workspace record, engine/provider 설치, `node_modules`, WebView와 활성 Rust build cache는 선택하지
  않는다.

### 검증 결과

- 시스템 Temp의 300MiB 이상 대형 테스트 폴더 9개 약 8.9GiB를 제거했다.
- 저장소의 `.build`, `.qa`, `.tmp`, `coverage`, `dist`, 구형 `release-artifacts`,
  `code_memory/build` 등 ignored/untracked output 11개 약 2.67GiB를 제거했다.
- test fixture 안에 생성된 C#/Java/Rust/C/C++의 `bin`, `obj`, `target`, `.cache`, IDE metadata와
  `db_memory/examples/shop-cache.sqlite`도 scoped `git clean` preview 뒤 제거했다.
- `D:\meeting-overlay-assistant`, `src`, `crates`, provider, 현재 desktop target,
  `ws-b74fcb1155c7a3e2/workspace.json`이 그대로 존재함을 재확인했다.
- 보수적 preview에서 남은 시스템 Temp 94개·약 2.38GiB는 모두 300MiB 미만이라 자동 삭제하지 않았다.

### 재발 시 점검 순서

1. 새 검증이 source tree 아래 고유 `*-final`, `*-fixed`, `*-debug` 폴더를 만드는지 본다.
2. 자동 테스트는 가능하면 RAII temp directory를 사용하고 process 종료 후 잔존 여부를 검사한다.
3. 유지할 golden fixture와 재생성 output을 같은 폴더에 두지 않는다.
4. 정리 전 preview, tracked file 0, 현재 process가 쓰지 않는 경로임을 확인한다.

### 남은 한계 또는 후속 gate

현재 남긴 94개 소형 Temp artifact는 무차별 삭제보다 안전을 우선한 결과다. 같은 prefix가 다시 계속
증가하면 생성 주체별 lifetime/cleanup을 고쳐야 하며, 단순히 크기 임계값을 낮추는 것으로 해결하지 않는다.

---

## TS-2026-08-08-39 — source excerpt를 일반 문장처럼 trim 검사하지 않는다

### 증상

정적 Fact Graph와 source evidence 저장은 끝났지만 AI 호출 전에
`InvalidText at input.excerpts[0].text: message must be trimmed, non-empty, and at most 32768 UTF-8 bytes`
오류로 의미 지도를 만들지 못했다. 화면에서는 입력이 너무 큰 문제처럼 보였다.

### 영향

정상적인 코드 들여쓰기나 파일 끝 줄바꿈이 있는 거의 모든 실제 source excerpt가 provider에 도달하기도
전에 거부될 수 있었다. 입력 크기를 줄이거나 파일로 바꿔도 해결되지 않는 계약 오류다.

### 잘못 짚기 쉬운 원인

오류 문구가 empty/trim/size 세 원인을 하나로 합쳐 표시했기 때문에 전체 packet 과대나 32KiB 초과로
오인하기 쉽다. producer의 excerpt 상한은 이미 24KiB였고 실제 실패는 `value.trim() != value`였다.

### 근본 원인

사람이 작성한 label/summary용 `validate_message`를 원본 source code에도 재사용했다. 일반 문장은 앞뒤
공백을 금지할 수 있지만 code indentation과 마지막 newline은 보존해야 할 증거 bytes다. 또한 typed
contract 상수는 v2인데 provider JSON Schema의 `schemaVersion` enum은 v1로 남아 있어, 앞 오류를
고친 뒤 실제 provider 단계에서 다시 실패할 결함도 함께 발견됐다.

### 적용한 수정

- source excerpt 전용 validator를 분리했다. non-whitespace 내용, 32KiB byte 상한, 안전한 newline/tab,
  금지 control byte만 검사하고 원문 indentation/newline은 보존한다.
- provider output schema version을 `BASE_SEMANTIC_SCHEMA_VERSION` 단일 상수에서 생성한다.
- packet compiler/prompt policy를 v2로 올려 구형 cache와 새 계약을 섞지 않는다.

### 검증 결과

선행 공백·다중 indentation·마지막 newline이 있는 실제 코드 형태가 byte 그대로 packet에 남는 회귀와,
JSON Schema enum이 typed contract version과 같은 회귀가 통과했다. 빈/공백-only, oversize, 금지 control
byte는 계속 거부한다.

### 재발 시 점검 순서

1. 오류의 empty/whitespace/byte-limit 원인을 각각 분리해 측정한다.
2. source bytes와 사용자-facing prose가 같은 validator를 공유하는지 확인한다.
3. producer 상한과 compiler 상한을 둘 다 확인한다.
4. Rust contract version과 외부 JSON Schema enum을 직접 비교하는 테스트를 둔다.

### 남은 한계 또는 후속 gate

excerpt가 유효하다는 것은 AI에 전부 보낼 가치가 있다는 뜻이 아니다. 전송 선택·분할·redaction은 별도
budget 계약이 소유하며, source validator에서 임의 truncation하지 않는다.

---

## TS-2026-08-08-40 — 큰 의미 입력을 파일 하나나 단일 AI 호출로 우겨 넣지 않는다

### 증상

Base 입력은 최대 192 region, region당 anchor 12개, source excerpt 48개×24KiB와 다수 boundary/TracePath를
한 요청에 직렬화할 수 있었다. 입력 파일로 전달하자는 대안도 나왔지만 CLI는 이미 stdin bytes로 받고
있어 model이 읽는 정보량은 줄지 않는다.

### 영향

대형 저장소에서 context 초과, 긴 대기 뒤 전체 실패, 한 부분 실패로 전체 재호출, 불필요한 source 원문
재전송이 발생할 수 있다. 임의 앞부분 truncation은 더 위험하며 일부 region을 누락한 지도를 완성본으로
위장하게 된다.

### 잘못 짚기 쉬운 원인

전송 매체(file/stdin)와 model context 양을 같은 문제로 보면 안 된다. 파일로 바꿔도 AI가 결국 같은
bytes를 읽는다. 반대로 region마다 완전히 독립 호출만 하고 바로 합치면 cross-region 의미와 전역 이름
중복을 잃는다.

### 근본 원인

기존 compiler에는 complete Base packet과 단일 provider call만 있었고, 결정적 partition·부분 검증·전역
reconciliation 계약이 없었다. 그래서 fail-closed 메시지만 있었고 실제 대형 실행 경로는 없었다.

### 적용한 수정

- static boundary relation 결속도를 우선하는 결정적·비중복 region partition planner를 추가했다.
- direct 4 region/96KiB, local 12 region/96KiB의 보수적 byte/개수 경계를 두고 단일 region 자체가
  초과하면 조용히 자르지 않고 실패한다.
- local packet은 자기 anchor/source excerpt와 내부 relation, partition 안에 완전히 포함된 TracePath만
  받으며 strict Base verifier를 각각 통과한다.
- provider process는 최대 2개만 병렬 실행한다. 실패한 local 작업만 새 one-shot 호출로 한 번 재시도하고,
  성공 결과는 packet digest 기준 최소 구조화 cache로 남긴다. raw prompt/response 대화는 저장하지 않는다.
- global reconciliation은 source 원문 없이 전체 region directory, 방향별 대표 boundary 6개, 검증된 local
  결과만 받는다. 결과는 원래 전체 Base packet verifier를 다시 통과해야 publish된다.
- Codex `--ephemeral`, Claude `--no-session-persistence`와 Claude tools empty를 테스트로 고정했다. 각 local과
  global 호출은 서로의 provider conversation을 이어받지 않는다.

### 검증 결과

입력 재정렬 2회에서 partition key·region membership·prompt bytes가 동일했고, 모든 region은 정확히 한
partition에만 포함됐다. source evidence 소유권 격리, global prompt source 원문 0, 전체 packet 최종 verifier
재사용, oversized single-region 명시 실패 회귀가 통과했다. compiler offline test와 Tauri provider flag/
오케스트레이션 test가 모두 통과했다.

### 재발 시 점검 순서

1. 전체 prompt 총량뿐 아니라 region/anchor/relation/trace/excerpt별 bytes를 측정한다.
2. partition 합집합이 전체 region과 같고 교집합이 비어 있는지 검사한다.
3. local 결과를 전체 지도처럼 publish하는 우회 경로가 없는지 확인한다.
4. global payload에 source excerpt가 다시 들어갔는지 확인한다.
5. provider args에 resume/continue가 없고 ephemeral/no-session-persistence가 있는지 확인한다.
6. 한 작업 실패 뒤 성공한 partition cache가 재호출되지 않는지 실제 provider gate로 측정한다.

### 남은 한계 또는 후속 gate

이 항목에서 남겼던 512KiB 단일 global reconciliation 한계는 TS-2026-08-09-90의 compact shuffle과
계층형 다단 Reduce로 해소했다. 192개 static region directory 상한과 실제 S/M/L 저장소의 시간·비용·semantic
stability 측정은 별도 보수 gate로 남는다.

---

## TS-2026-08-08-41 — provider ZIP은 Windows PowerShell 5.1 호환 압축 enum을 사용한다

### 증상

데스크톱 빌드의 provider bundle 생성 단계에서
`The property 'SmallestSize' cannot be found on this object` 오류가 발생하고, 분석 공급자 ZIP 폴더에는
0-byte 또는 일부 묶음만 남았다.

### 영향

Rust·프론트 코드가 모두 정상이어도 설치용 언어 공급자를 만들지 못해 데스크톱 빌드가 중단된다. 실패
도중 기존 bundle root를 다시 만들기 때문에 오류를 무시하면 불완전한 provider 자산을 다음 실행이
사용할 위험도 있다.

### 잘못 짚기 쉬운 원인

분할 AI 분석이나 provider 실행 실패가 아니다. `CompressionLevel.SmallestSize`는 새 .NET 런타임에는
있지만 Windows PowerShell 5.1이 사용하는 .NET Framework enum에는 없다.

### 근본 원인

빌드 스크립트가 지원 PowerShell/.NET 범위보다 새로운 압축 enum을 직접 참조했다. 개발 환경에서만
존재하는 enum을 패키징 계약으로 고정한 호환성 결함이다.

### 적용한 수정

- 모든 지원 .NET 런타임에 존재하는 `CompressionLevel.Optimal`을 단일 압축 정책으로 사용한다.
- 실패 뒤 provider bundle 8개를 처음부터 다시 생성하고 서명 매니페스트를 검증한다.
- 내부 배포 모드의 Tauri `--no-bundle` 빌드로 provider 검증부터 최종 실행 파일 생성까지 확인한다.

### 검증 결과

서명된 provider pack 8개가 다시 생성됐고 `providers-manifest.json` 서명 검증이 통과했다. 프론트 production
build, 엔진/notice/dependency inventory gate, Tauri release profile을 거쳐
`src-tauri/target/release/codebase-workspace.exe`가 생성됐다.

### 재발 시 점검 순서

1. 실패한 PowerShell의 `$PSVersionTable`과 `CompressionLevel` enum 이름을 확인한다.
2. bundle root에 0-byte ZIP 또는 manifest/signature 누락이 있는지 확인한다.
3. 개별 ZIP 존재만 보지 말고 catalog signature 검증을 실행한다.
4. 배포 가능 공개 서명 키가 없는 로컬 검증은 internal build로 수행하고, 이를 재배포 빌드로 오인하지 않는다.

### 남은 한계 또는 후속 gate

이번 통과는 개발 서명 자산을 검증한 internal build다. 외부 배포 설치본은 별도의 release 공개키·라이선스
gate·installer smoke를 통과해야 한다.

---

## TS-2026-08-08-42 — PATH 첫 Codex가 아니라 설치된 최신 호환 CLI를 작업 단위로 고정한다

### 증상

정적 코드 사실 저장 뒤 AI 의미 분할 1/16에서 Codex가
`failed to load models cache: missing field base_instructions`로 시작 즉시 종료했다. 화면에는
`OpenAI Codex v0.142.5`가 표시됐다.

### 영향

프롬프트·분할·모델 자체가 정상이어도 첫 local 작업에서 의미 지도가 전부 중단된다. 같은 구형 실행 파일을
다시 고르면 모든 partition과 재시도가 같은 이유로 실패한다.

### 잘못 짚기 쉬운 원인

`models_cache.json`은 대화 기록이 아니며 JSON 손상도 아니었다. 215,310 bytes JSON은 정상 파싱됐고
모델 9개 모두 신형 `model_messages`를 가졌지만 구형 `base_instructions`는 없었다. `--ephemeral`이나
분할 입력 크기와도 무관하다.

### 근본 원인

- PATH의 visible standalone CLI는 `0.142.5`였다.
- Codex Desktop이 실제 사용 중인 CLI와 cache client version은 `0.147.0` 계열이었다.
- 기존 desktop adapter는 `where codex` 첫 줄만 선택했다.
- 신형 Codex가 공유 cache를 새 schema로 쓴 뒤 구형 CLI가 이를 구형 typed schema로 읽어 실패했다.

### 적용한 수정

- 앱 시작 시 기존 Codex/Claude 설치물을 한 번 탐색하는 `ProviderRegistry`를 추가했다.
- Windows에서는 PATH/standalone 후보와 기존 Codex Desktop managed runtime 후보를 직접 실행해 실제
  `--version`을 비교한다. 파일을 복사·설치·수정하지 않는다.
- Codex는 공유 model cache보다 오래되지 않은 후보 중 최신 실행 파일만 등록한다. 호환 후보가 없으면
  분석 16개를 시작하기 전에 버전 충돌을 명시한다.
- 분석 시작 시 등록된 파일과 cache 호환성만 한 번 재검사한다. 변경됐을 때만 registry를 한 번 갱신한다.
- local partition, 실패 재시도, global reconciliation은 하나의 `ResolvedProvider` snapshot과 절대경로를
  끝까지 공유한다. 분할마다 PATH 탐색이나 version probe를 반복하지 않는다.

### 검증 결과

실제 PC에서 registry는 구형 `0.142.5` 대신 이미 설치된
`C:\Users\plosind\AppData\Local\OpenAI\Codex\bin\cfac6bda2d141e07\codex.exe`
(`codex-cli 0.147.0-alpha.6.5`)를 선택했고 cache core `0.147.0` 이상 gate를 통과했다. 안정/preview version
파싱, 최신 호환 후보 선택, 구형-only 명시 거부 회귀가 통과했다. 이 검증은 `--version`만 실행했고 외부
AI 요청은 보내지 않았다. Tauri 전체 66 passed/4 environment-only ignored, clippy `-D warnings`, frontend
6/6·typecheck·lint·production build와 internal release-profile 실행 파일 빌드도 통과했다.

### 재발 시 점검 순서

1. UI가 표시한 provider executable/version과 `models_cache.json.client_version`을 비교한다.
2. PATH 첫 줄만 보지 말고 registry가 선택한 절대경로를 확인한다.
3. Codex 업데이트 뒤 managed runtime 경로가 바뀌었으면 다음 분석 경계에서 registry가 갱신되는지 본다.
4. 같은 분석의 local/retry/global 로그가 하나의 runtime version을 사용하는지 확인한다.
5. cache 삭제로 숨기지 말고 실행 파일과 cache writer의 버전 계약을 맞춘다.

### 남은 한계 또는 후속 gate

Codex Desktop의 managed runtime 배치 구조가 향후 바뀌면 공식 visible standalone 경로가 최신 상태여야 하며,
호환 CLI를 찾지 못할 때는 명시 실패한다. 실제 의미 분석 재실행은 외부 모델 비용을 발생시키므로 자동
회귀에서 수행하지 않고 인증된 opt-in E2E gate로 유지한다.

---

## TS-2026-08-08-43 — 여러 region의 area ID는 제어문자 delimiter로 합치지 않는다

### 증상

16개 local 의미 분석과 첫 partition 재시도까지 끝난 뒤 다음 오류로 전체 의미 지도 게시가 막혔다.

```text
InvalidProviderOutput at areas[...].areaId:
identity component contains a forbidden control character
```

첫 결과와 재시도에서 proposal key는 달랐지만 오류는 같았다.

### 영향

둘 이상의 effective region을 포함한 의미 영역은 provider 답변 내용과 무관하게 ID 생성 단계에서 항상
실패할 수 있었다. 더 나쁘게는 batch의 모든 원시 결과를 먼저 모은 뒤 index 순서로 검증했기 때문에,
앞 partition 하나가 실패하면 이미 끝난 뒤 partition의 성공 결과도 cache하기 전에 반환했다. 대형 저장소는
수분의 모델 실행과 비용을 쓰고도 재실행 때 처음부터 다시 시작했다.

### 잘못 짚기 쉬운 원인

화면에 proposal key만 보이므로 model이 area ID에 zero-width/control 문자를 넣었다고 오해하기 쉽다.
그러나 provider output contract에는 최종 `areaId` 필드가 없다. AI는 제한된 ASCII `proposalKey`만 만들고,
최종 `SemanticAreaId`는 verifier가 snapshot·level·parent·member 집합에서 파생한다. CLI, prompt schema,
모델 재시도는 이 실패의 원인이 아니었다.

### 근본 원인

`create_area_id`가 여러 `RegionId`를 `join("\n")`으로 한 문자열에 합쳤다. 같은 호출의
`SemanticAreaId::from_components`는 각 identity component에 `char::is_control`이 있으면 거부한다.
즉 verifier가 금지 문자를 직접 만든 뒤 이를 provider 오류로 분류한 자기모순이었다.

오케스트레이션도 `run_provider_batch -> Vec<Result<...>> -> index 순서 검증` 구조라 성공 결과의 즉시
검증·저장이 불가능했다.

### 적용한 수정

- member ID를 정렬하고 각각 별도의 length-prefixed identity component로 hash한다. 임의 delimiter와
  escaping은 사용하지 않는다. 단일 member의 기존 ID 입력 순서는 유지하고, 과거 생성 불가능했던 다중
  member ID만 정상 생성한다.
- 파생 ID 생성 자체가 실패하면 `InvalidProviderOutput`이 아니라 `InvalidPacket`의 `derivedAreaId`로
  분류해 AI 책임과 내부 불변식 실패를 구분한다.
- provider batch는 완료 결과를 callback으로 즉시 전달한다. 각 성공 결과를 바로 strict verifier로
  검사하고 immutable partition cache에 저장한다.
- 첫 실행이 실패한 partition만 별도 batch로 한 번 재시도한다. 최종 실패가 있어도 다른 성공 cache는
  남으며, 불완전한 전체 지도는 계속 publish하지 않는다.
- 진행 문구에 initial 처리 수, 검증 완료 수, 재시도 처리 수를 분리해 표시한다.

### 검증 결과

두 region이 한 L0 area의 effective member가 되는 회귀를 추가했다. assignment 입력 순서를 뒤집어도 같은
area ID가 생성된다. semantic compiler 22/22, Tauri 66 passed/4 environment-only ignored가 통과했다.
같이 발견된 frontend workspace stale-ref 경쟁 조건을 수정한 뒤 frontend도 6/6을 통과했다. 실제 외부
provider 재분석은 비용이 발생하므로 이 시점의 자동 회귀에는 포함하지 않았다.

### 재발 시 점검 순서

1. 오류 path가 provider가 실제 출력하는 필드인지 verifier가 파생하는 필드인지 먼저 구분한다.
2. stable ID 집합을 delimiter 문자열로 합치지 말고 각 항목을 별도 length-prefixed component로 넣는다.
3. 한 partition 실패 뒤 다른 성공 partition cache 파일이 실제로 남았는지 확인한다.
4. 재실행에서 성공 cache는 호출하지 않고 실패 partition만 다시 호출하는지 provider 로그로 측정한다.
5. local 전체가 승인되기 전 global reconciliation 또는 semantic pointer가 게시되지 않는지 확인한다.

### 남은 한계 또는 후속 gate

실제 16-partition 저장소를 새 실행 파일로 재분석해 local cache hit/miss, 총 호출 수, 전역 통합 성공,
wall time을 별도 opt-in E2E 영수증으로 남겨야 한다. 현재 수정은 correctness와 실패 보존을 닫았지만
provider 지연 자체를 줄이지는 않는다.

---

## TS-2026-08-08-44 — workspace state와 stale-result guard를 같은 turn에 갱신한다

### 증상

frontend vertical slice에서 분석 command는 성공했지만 새 MapView를 호출하지 않거나 결과를 버려
빈 canvas가 유지되는 간헐적 실패가 발생했다.

### 영향

workspace가 화면에 이미 선택되어 분석 버튼을 누를 수 있는데도, 빠르게 분석을 시작하면 정상 결과가
다른 workspace의 오래된 결과로 오인되어 표시되지 않을 수 있었다.

### 근본 원인 및 수정

`activeWorkspaceId` state는 즉시 바뀌지만 stale async 결과를 막는 `activeWorkspaceIdRef`는 뒤따르는
effect에서 갱신됐다. 그 사이 시작한 분석이 완료되면 ref가 아직 `null` 또는 이전 ID일 수 있었다.
초기 workspace 선택, 사용자 선택, upsert 모두에서 state와 ref를 같은 turn에 갱신하도록 바꿨다.

### 검증 결과

기존 분석 vertical-slice 회귀가 다시 통과했고 frontend 전체는 6/6이다. engine fixture가 test suite에
섞였다는 추정은 사실이 아니었다. `vitest.config.ts`의 include는 `src/**/*.{test,spec}.{ts,tsx}`로 제한돼
있다. Fluent `.analysis-button`과 `.provider-dialog` style도 현재 `src/styles.css`에 존재한다.

### 재발 시 점검 순서

1. 화면 state와 async stale guard가 서로 다른 effect 시점에 갱신되는지 본다.
2. command 호출 수뿐 아니라 후속 `getMapView` 호출과 state 반영을 함께 검증한다.
3. test discovery 문제를 주장하기 전에 실제 실패 test와 `vitest.config.ts` include를 확인한다.

### 남은 한계 또는 후속 gate

분석 중 workspace 전환·삭제·재선택 시나리오를 추가해 오래된 결과는 버리되 현재 결과는 잃지 않는지
확장 회귀가 필요하다.

---

## TS-2026-08-08-45 — LSP 빈 응답마다 retry sleep을 반복하지 않는다

### 증상

두 파일뿐인 Rust analysis job이 각각 약 190초, 125초 걸렸고 대형 Python provider는 884개 파일에서
약 704초가 걸렸다. 사용자는 provider 단계가 55~65%에서 멈춘 것처럼 보였다.

### 영향

source 수와 exact query position 수가 늘수록 실제 서버 계산보다 고정 대기가 선형으로 누적됐다. 지원
언어와 파일이 많은 정상 대형 저장소에서 최초 분석이 수십 분으로 늘어날 수 있었다.

### 잘못 짚기 쉬운 원인

LSP 서버 자체가 느리거나 대형 source parsing이 전부를 차지한다고 보기 쉽다. 그러나 계측 뒤 Rust의
definition response wall time은 전체 776개 요청에 10ms뿐이었다. 대부분은 빈 응답마다 세 번 반복한
`250ms` client sleep이었다.

### 근본 원인

각 source position이 `definition`을 요청하고, 결과가 비면 그 position 안에서 독립적으로 기다렸다.
즉 서버 부하를 완화하려던 retry backoff가 `position 수 × round 수 × 250ms`의 직렬 지연으로 증폭됐다.

### 적용한 수정

- exact CST/provider query position을 먼저 수집한다.
- 최대 16개 JSON-RPC 요청을 bounded batch로 보내고 response id로 원래 position에 결합한다.
- 기존과 같은 세 번의 provider-only round를 전체 position에 공유한다. round 사이의 wait은 전체에서
  한 번만 발생한다.
- 결과는 URI+position cache에 저장하고 기존 relation builder가 소비한다. 이름 유사도나 lexical target
  추정은 추가하지 않는다.
- method별 batch/request/error/total/max wall time receipt를 남긴다.

### 검증 결과

동일 실제 저장소에서 Python 884 files는 703,743ms에서 56,316ms로, Rust 두 job은
189,847ms→7,264ms와 125,106ms→7,236ms로 줄었다. 최종 canonical file coverage는 1,037로 같고,
node/edge/evidence는 오히려 22/47/47개 증가했다. dangling endpoint와 evidence 없는 confirmed는 0이다.
별도 cold 실험에서 batch 64는 Python 55,603~57,080ms로 batch 16의 56,316ms와 사실상 같았고 canonical
digest/count도 동일했다. 따라서 단순 batch 확대는 추가 최적화로 채택하지 않았다.

### 재발 시 점검 순서

1. provider receipt에서 job별 wall time을 확인한다.
2. LSP receipt에서 request count가 아니라 method별 실제 wall time과 batch 수를 본다.
3. 고정 sleep이 file·symbol·position loop 안에 있는지 찾는다.
4. batching 전후 exact query position 집합과 canonical digest/count를 대조한다.
5. 빠른 결과를 위해 definition을 생략하거나 추정 relation을 만들지 않았는지 확인한다.

### 남은 한계 또는 후속 gate

Python definition 11,396개가 41,973ms로 남은 cold 병목이다. language server의 공식 bulk API 또는
정확한 compiler index 재사용이 가능할 때만 더 줄인다.

---

## TS-2026-08-08-46 — TypeScript indexer의 파일 크기 ceiling을 scheduler shard만 보고 정하지 않는다

### 증상

SCIP TypeScript가 `WorkspaceCanvas.jsx` 같은 정상 source를 `max-file-byte-size` 초과로 건너뛰었다.
로그에는 job마다 약 2.93KB 또는 16.8KB처럼 비정상적으로 작은 threshold가 표시됐다.

### 영향

Source Census는 파일을 승인하고 coverage 분모에도 넣었지만 semantic indexer가 실제 symbol과 relation을
누락할 수 있었다. 속도는 빨라 보이지만 제품 지도는 큰 핵심 파일을 잃는 정확도 회귀였다.

### 잘못 짚기 쉬운 원인

큰 파일 자체가 비정상이거나 현재 scheduler shard 밖의 파일이므로 무관하다고 보기 쉽다. 그러나
scip-typescript는 전달한 shard보다 넓은 상위 tsconfig project member를 읽는다.

### 근본 원인

`--max-file-byte-size`를 현재 scheduler shard에 포함된 파일의 최대 크기로만 계산했다. indexer의 실제
project scope와 ceiling 계산 scope가 달랐다.

### 적용한 수정

Source Census가 승인한 프로젝트 전체 source의 최대 byte 수를 `LanguageJob` 계약에 넣고, 모든
TypeScript/JavaScript SCIP job과 fallback runner가 `max(project-wide admitted max, shard max)`를 사용한다.
제외된 binary/generated/vendor 파일을 다시 허용하는 변경은 아니다.

### 검증 결과

같은 cold 실행에서 size threshold skip warning이 0이 됐다. file coverage 1,037은 유지되고 nodes +22,
edges +47, evidence +47, typed gaps -4로 누락 데이터가 복구됐다. cold와 warm snapshot/digest/count도
완전히 같았다.

### 재발 시 점검 순서

1. Source Census 승인 파일의 최대 byte 수와 runner 인자를 비교한다.
2. provider가 project config를 통해 shard 밖의 어떤 file을 읽는지 확인한다.
3. `skipping file` warning을 성능 최적화 성공으로 취급하지 않는다.
4. coverage count뿐 아니라 node/edge/evidence와 대표 대형 source symbol을 대조한다.

### 남은 한계 또는 후속 gate

새 JS/TS provider를 도입하면 runner의 project discovery scope와 Source Census admission scope가 일치하는지
별도 large-source gate로 검증해야 한다.

---

## TS-2026-08-08-47 — 선택 상세마다 immutable Fact Bundle 전체를 다시 읽지 않는다

### 증상

지도에서 node 하나를 선택할 때마다 SQLite의 nodes, edges, evidence, file coverage, capability receipts,
gaps 여섯 테이블을 모두 `Vec`으로 만들고 bundle digest를 다시 검증했다.

### 영향

분석 자체를 빠르게 해도 수만 symbol 규모 저장소에서 클릭과 inspector 갱신이 I/O·allocation에 묶여
제품이 계속 느리게 느껴졌다.

### 근본 원인

publish된 bundle은 snapshot 동안 immutable인데 read model은 매 command를 독립 cold load로 취급했다.
workspace process state에는 provider registry만 있었고 verified snapshot cache가 없었다.

### 적용한 수정

workspace별 검증 완료 `CanonicalFactSnapshot`을 `Arc`로 보관하는 bounded cache를 추가했다. 최대 2개만
resident로 두며 pointer identity, bundle path, file length/mtime가 바뀌면 cache를 버리고 기존의 전체
digest/integrity/row validation을 다시 수행한다. 같은 immutable snapshot의 map/selection query는 Arc를
공유한다.

### 검증 결과

연속 두 read가 같은 `Arc`를 재사용하는 회귀를 추가했다. Tauri 전체 68 passed, 4 environment-only
ignored이며 snapshot 변경·bundle 누락·integrity 불일치 검증은 기존대로 fail-closed다.

### 재발 시 점검 순서

1. map selection command가 full snapshot loader를 호출하더라도 cache hit인지 확인한다.
2. 새 publish 뒤 이전 Arc가 반환되지 않는지 pointer와 bundle metadata를 바꿔 검증한다.
3. cache miss에서 digest/integrity/row validation이 생략되지 않았는지 확인한다.
4. resident snapshot 수가 무제한으로 늘지 않는지 확인한다.

### 남은 한계 또는 후속 gate

현재 cache는 process-local이며 앱 재시작 첫 read는 다시 검증한다. 향후 node 단위 SQLite query를 도입해도
snapshot identity와 fail-closed 검증 계약은 유지해야 한다.

---

## TS-2026-08-08-48 — `scip-python` Windows empty index를 빠른 성공으로 오판하지 않는다

### 증상

`@sourcegraph/scip-python@0.6.6`을 Windows에서 실행하면 시작 시
`new RegExp(path.sep, "g")`가 invalid regular expression으로 실패했다. 실험용 전역 `path.sep` 변경 뒤에는
명령이 3.172초 만에 끝났지만 파일 0개, 64-byte SCIP만 생성됐다.

### 영향

42초 Python definition 병목을 없애기 위한 batch index 후보를 검증할 때, path/프로젝트 발견 실패로 생긴
빈 index를 극적인 성능 향상으로 잘못 기록할 수 있다. 이를 publish하면 Python fact 전체가 사라진다.

### 잘못 짚기 쉬운 원인

SCIP 형식이나 Pyright semantic analysis가 Windows를 지원하지 않는 문제로 보기 쉽다. 직접 재현된 첫
원인은 package code가 Windows의 단일 backslash separator를 regex metacharacter로 escape하지 않은 것이다.

### 근본 원인

package의 `new RegExp(path.sep, "g")`가 separator를 regex-safe string으로 만들지 않는다. `path.sep` 전역
값을 이중 backslash로 바꾸면 그 한 정규식은 통과하지만 다른 path join/normalization 코드도 같은 값을
사용해 `D:\...`를 `\D:\...`처럼 오염시킨다.

### 적용한 수정

production 수정은 적용하지 않았다. 전역 monkey patch 방식이 잘못됐음을 확인하고 실험 결과를 폐기했다.
이후 해당 정규식 구성 한 곳만 escape한 isolated source patch로 shadow 실험을 수행했다. 이 patch와 package는
production provider pack에 넣지 않았다.

### 검증 결과

원본 package는 Windows에서 즉시 예외를 재현했고, 전역 patch 결과는 project file 0개와 64-byte output을
확인했다. 따라서 3.172초는 유효한 분석 성능이 아니다. 최소 patch 실험은 실제 저장소의 승인 Python
source 884/884를 처리했고 direct index는 약 13.7초였지만, current Pyright LSP truth와 비교했을 때
definition 38개, workspace occurrence 6,072개, relation 139개를 보존하지 못했다. 같은 package dependency
tree의 audit에서도 53개 취약점 경고가 나왔다.

### 재발 시 점검 순서

1. wall time보다 먼저 indexed file count와 SCIP output size를 확인한다.
2. Source Census 승인 file count와 provider document count를 대조한다.
3. Windows drive/UNC 경로가 변형되지 않았는지 확인한다.
4. empty index를 success로 publish하지 않고 provider gap 또는 검증된 fallback을 사용한다.
5. 새 provider의 normalized semantic fact set을 기존 provider와 shadow diff한다.

### 남은 한계 또는 후속 gate

빠르다는 사실은 확인했지만 production 승격 gate는 실패했다. Pyright 최신 세대로 port한 maintained fork,
dependency 정리, Windows package 서명, canonical fact parity와 human ground truth를 모두 통과하기 전에는
현재 Pyright LSP를 유지한다.

---

## TS-2026-08-08-49 — provider symbol 문자열이나 총 fact 수를 정확도로 오판하지 않는다

### 증상

같은 source를 두 semantic provider로 분석했는데 definition/call이 실제로 같은 위치를 가리켜도 SCIP symbol
문자열과 raw fact 수가 달라 대규모 diff가 발생했다. 반대로 candidate가 fact를 더 많이 만들면 더 정확해
보였다.

### 영향

정확한 새 provider를 거짓 회귀로 폐기하거나, external/unresolved/중복 fact가 많은 provider를 더 정확한
것으로 승격할 수 있었다.

### 잘못 짚기 쉬운 원인

SCIP schema를 쓰면 모든 indexer의 symbol identity와 occurrence granularity도 같을 것이라고 보기 쉽다.
SCIP은 exchange protocol이지 각 provider의 내부 symbol 생성·range 선택을 완전히 동일하게 만드는 규칙이
아니다.

### 근본 원인

비교 단위가 사용자에게 보여줄 사실이 아니라 provider 고유 serialization과 symbol 문자열이었다.

### 적용한 수정

`compare-scip`을 추가해 workspace definition을 exact `(path, range)` locator로 rebasing하고 occurrence와
relation endpoint도 그 locator로 비교한다. current fact regression과 candidate-only extension을 분리하고,
candidate-only fact는 자동 confirmed 승격하지 않는다. report는 독립 ground truth 전에는
`productionEligible: false`를 강제한다.

### 검증 결과

같은 의미를 서로 다른 symbol scheme으로 표현한 unit test가 definition/occurrence/relation F1 1.0과 같은
semantic digest를 낸다. TypeScript/JavaScript same-provider shadow도 normalization 보정 후 각각
22/22·40/40·15/15와 26/26·49/49·15/15 current fact를 보존했다.

### 재발 시 점검 순서

1. provider symbol 문자열 diff가 아니라 normalized locator diff를 본다.
2. candidate-only fact와 baseline-only regression을 따로 센다.
3. 총 fact 수가 아니라 representative evidence를 human/canonical ground truth로 검토한다.
4. raw shadow 통과만으로 production 승격하지 않는다.

### 남은 한계 또는 후속 gate

두 provider가 모두 같은 사실을 잘못 만들면 shadow 비교만으로 발견할 수 없다. reviewed ground-truth corpus와
Canonical Fact Bundle parity는 계속 별도 gate다.

---

## TS-2026-08-08-50 — C# restore 생략을 무해한 warm-cache 최적화로 취급하지 않는다

### 증상

`scip-dotnet 0.2.14` fixture를 `--skip-dotnet-restore`로 실행하면 빠르게 끝났지만 실제 `Add(...)` 호출
occurrence와 relation이 하나씩 사라졌다.

### 영향

분석 시간 약 1초를 줄이는 대신 사용자가 보는 실행 관계가 조용히 누락됐다. project/config digest가 같아도
속도 캐시가 semantic truth를 바꿀 수 있었다.

### 잘못 짚기 쉬운 원인

restore는 package download 준비 단계일 뿐 source semantic analysis와 무관하다고 보기 쉽다. 그러나
Roslyn project model과 reference resolution은 restore 산출물에 의존한다.

### 근본 원인

restore 완료 상태를 파일 digest cache로 대체하고 indexer에 skip flag를 넘겼다. 이 cache는 indexer가
필요로 하는 전체 MSBuild/Roslyn 상태를 증명하지 못했다.

### 적용한 수정

production runner의 `--skip-dotnet-restore`, restore-state file, project-config restore cache를 제거했다.
restore를 성능 선택지가 아니라 correctness boundary로 문서화했다.

### 검증 결과

동일 fixture에서 skip은 definition 17/17, occurrence 24/25, relation 8/9였고 restore 실행은
17/17, 25/25, 9/9를 보존했다. restore 실행의 normalized shadow F1은 세 집합 모두 1.0이다.

### 재발 시 점검 순서

1. C# command line에 `--skip-dotnet-restore`가 없는지 확인한다.
2. 속도 비교 때 definition뿐 아니라 call occurrence/relation을 대조한다.
3. restore 비용은 NuGet/MSBuild가 보장하는 정상 cache로 줄이고 제품이 자체적으로 생략하지 않는다.

### 남은 한계 또는 후속 gate

대형 solution에서 restore overhead는 별도 측정해야 한다. 단, 해결책은 정확도에 영향을 주는 skip flag가
아니라 persistent dependency cache와 incremental project scheduling이어야 한다.

---

## TS-2026-08-08-51 — serialized byte 결정성과 semantic 결정성을 분리한다

### 증상

동일 Go fixture를 `scip-go`로 두 번 분석했을 때 SCIP file SHA-256이 달랐다. byte digest만 보면 provider가
비결정적으로 보였다.

### 영향

의미 결과가 완전히 같은 후보를 잘못 탈락시키거나, 반대로 byte가 같다는 이유로 semantic 결과를 충분히
검증하지 않을 수 있었다.

### 근본 원인

protobuf ordering/metadata/serialization과 제품이 사용하는 normalized facts를 같은 identity로 취급했다.

### 적용한 수정

document path, definition locator, normalized occurrence, normalized relation의 정렬된 집합으로
`semanticFactDigest`를 계산한다. raw candidate SHA-256도 별도로 남긴다.

### 검증 결과

Go candidate 두 파일의 byte hash는 달랐지만 semantic digest는 동일했다. 이는 provider 채택을 뜻하지
않는다. current fact 보존 gate는 별도로 실패했으며 두 종류의 결정성이 독립 판정됨을 확인했다.

### 재발 시 점검 순서

1. raw file digest와 semantic fact digest를 모두 기록한다.
2. byte mismatch가 나면 normalized fact diff를 먼저 확인한다.
3. semantic digest가 같아도 packaging/version/config provenance가 같은지 확인한다.

### 남은 한계 또는 후속 gate

semantic digest는 현재 comparison schema의 fact 집합만 포함한다. Canonical Language IR/Fact Bundle에 새
필드가 추가되면 digest projection도 함께 version-up해야 한다.

---

## TS-2026-08-08-52 — 공식 indexer가 존재해도 Windows product provider인 것은 아니다

### 증상

`scip-java`는 실행 가능한 release asset이 있었지만 내부에서 literal `mvn`을 `ProcessBuilder`로 실행해
Windows의 `mvn.cmd`를 찾지 못하고 `CreateProcess error=2`로 실패했다. `scip-clang`은 공식 Windows binary
release 자체가 없었다.

### 영향

GitHub release 존재 여부만 보고 provider pack에 추가하면 사용자 PC에서 분석 첫 단계가 실패한다. 임의
shim이나 자체 build를 끼우면 서명·업데이트·license·resource budget 책임까지 제품으로 넘어온다.

### 잘못 짚기 쉬운 원인

Java가 cross-platform이고 C/C++ source가 Windows에서 빌드되므로 indexer launcher/binary도 자동으로
Windows product-ready라고 보기 쉽다.

### 근본 원인

언어 지원과 최종 배포 artifact 지원을 구분하지 않았다. Java 후보는 platform command naming 경계를,
C/C++ 후보는 upstream release platform과 2GB/core 수준의 resource requirement를 넘지 못했다.

### 적용한 수정

production registry는 Java JDTLS와 C/C++ clangd를 유지했다. candidate spike는 signed native launcher,
checksum, update path, memory/cancellation, compile-context gate 전까지 opt-in으로도 연결하지 않는다.

### 검증 결과

검증된 Maven 3.9.16 ZIP을 공급해도 Windows candidate가 compiler analysis 전에 같은 process creation
오류로 실패했다. scip-clang 공식 문서의 binary platform도 Linux x86_64/macOS arm64로 확인했다.

### 재발 시 점검 순서

1. release asset이 target OS/architecture를 직접 지원하는지 확인한다.
2. launcher가 `cmd`/PowerShell wrapper 없이 실제 executable을 찾는지 확인한다.
3. checksum/signature/license와 peak memory/cancellation을 provider gate에 포함한다.
4. 실행 불가 후보는 정확도 0점으로 단정하지 말고 packaging-blocked로 분리한다.

### 남은 한계 또는 후속 gate

upstream Windows launcher/binary가 제공되거나 제품이 유지 가능한 signed adapter를 만들 때 다시 shadow한다.

---

## TS-2026-08-08-53 — language server용 최소 SDK를 package indexer용 full SDK로 오판하지 않는다

### 증상

제품의 bundled Dart runtime은 Analysis Server를 실행했지만 `dart pub`은
`Unable to find snapshot: dartdev_aot.dart.snapshot`으로 실패했다.

### 영향

`scip_dart` candidate를 production에 연결하면 Analysis Server가 정상인 설치에서도 package activation과
dependency resolution을 수행하지 못한다.

### 잘못 짚기 쉬운 원인

`dart.exe`가 존재하고 LSP가 동작하면 full Dart SDK가 설치됐다고 보기 쉽다.

### 근본 원인

현재 provider pack은 LSP에 필요한 snapshot만 포함한 최소 runtime이고, package CLI와 pub cache 계약은
의도적으로 포함하지 않는다.

### 적용한 수정

shadow는 system full Dart SDK와 isolated `PUB_CACHE`, fixture 복사본에서만 수행했다. production provider는
Dart Analysis Server LSP를 유지하고 source tree나 사용자 global cache를 오염시키지 않았다.

### 검증 결과

candidate는 두 번의 byte/semantic digest가 같았고 definition 11/11을 보존했지만 occurrence 17/18,
relation 5/6이었다. 실제 constructor 한 건이 `CONSTRUCTS`에서 `REFERENCES`로 downgrade됐다.

### 재발 시 점검 순서

1. bundled `dart pub --version` 또는 package activation을 별도 검사한다.
2. provider가 요구하는 analyzer SDK 범위와 product SDK 버전을 비교한다.
3. 임시 `PUB_CACHE`와 복사 fixture를 써 source/global environment를 오염시키지 않는다.
4. definition count만 보지 말고 constructor/call/type relation kind를 대조한다.

### 남은 한계 또는 후속 gate

full Dart SDK 배포는 provider pack 크기·업데이트·보안 범위를 넓힌다. relation parity와 packaging 이득이
그 비용을 넘는다는 근거가 생길 때만 재검토한다.

---

## TS-2026-08-09-54 — 서로 다른 TypeScript project config를 한 Program으로 합치지 않는다

### 증상

ESLint의 여러 작은 config를 한 scip-typescript process로 합치자 cold가 246.715초에서 119.668초로 줄었지만
`CustomParserServices.program` field와 Declares/type relation/evidence가 각각 1개씩 사라졌다.

### 근본 원인

서로 다른 analysis root와 compiler option을 가진 TypeScript Program을 한 execution root로 접었다. process
global cache 문제가 아니어서 `--no-global-caches`로도 고쳐지지 않았다.

### 적용한 수정과 검증

cross-root batching은 `CODE_MEMORY_EXPERIMENTAL_TS_MULTI_CONFIG_BATCH=1` shadow에만 남겼다. 기본 경로는
cold/warm exact semantic·bundle digest가 같은 config별 실행을 유지한다. 같은 개수 비교가 아니라 누락 ID와
source evidence를 직접 대조했다.

### 재발 시 점검 순서

1. config별 direct file membership을 확인한다.
2. node/edge/evidence ID 집합을 exact diff한다.
3. 한 process 최적화는 diff 0일 때만 기본값으로 승격한다.

---

## TS-2026-08-09-55 — TypeScript transitive Program source를 direct config member로 소유하지 않는다

### 증상

NestJS에서 integration config가 `packages/core` 같은 dependency source를 먼저 소유했고, 같은 파일 집합의
cache가 이후 올바른 package tsconfig에서도 재사용될 수 있었다.

### 근본 원인

project model이 `Program.getSourceFiles()` 전체를 config의 direct member로 기록했고 language cache identity에
planned scope와 실제 provider config 내용이 없었다.

### 적용한 수정과 검증

direct member는 `parseJsonConfigFileContent().fileNames`로 한정하고 transitive source는 import/call evidence
수집에만 사용한다. cache key에는 stable execution scope와 provider config bytes를 포함했다. NestJS cold/warm
semantic·bundle digest가 같고 CatsController의 route/call/type evidence를 실소스와 대조했다.

---

## TS-2026-08-09-56 — config 없는 TS/JS 분석이 선택한 source tree에 파일을 쓰지 않게 한다

### 증상

Prometheus 분석 중 scip-typescript `--infer-tsconfig`가 Go 저장소의
`web/ui/module/lezer-promql/tsconfig.json`을 새로 만들었다.

### 영향과 근본 원인

읽기 전용 분석이 사용자 source를 변경했고 source-stability gate도 실패했다. infer flag의 작업 디렉터리를
선택한 저장소로 둔 것이 원인이다.

### 적용한 수정과 검증

configless TS/JS shard는 provider work directory에 정확한 파일 목록을 가진 격리 source-only config를 만들고
그것만 사용한다. 생성된 audit 파일을 제거한 뒤 Prometheus `git status --short`가 빈 상태이고 분석의
source-stability receipt가 unchanged임을 확인했다.

---

## TS-2026-08-09-57 — Rust 공개 impl 메서드를 대형 workspace 관계 추적에서 제외하지 않는다

### 증상

Tokio provider 원본에는 `Runtime::spawn`, `spawn_blocking`, `block_on` 정의가 있었지만 call relation은 0건이고
canonical graph에서도 메서드가 pruning됐다.

### 근본 원인

대형 workspace boundary가 `pub` 여부 외에 선언 줄이 column 0인지 요구했다. impl 안의 모든 메서드는
들여쓰기되므로 실제 공개 API를 속도 최적화가 잘라냈다.

### 적용한 수정

exact LSP symbol과 source line에서 `pub`/`pub(...)`가 확인되면 공개 impl 메서드도 map boundary로 유지한다.
Rust 전용 cache marker와 공개/비공개 메서드 회귀 테스트를 추가했다. 보정된 Tokio cold parity는 감사 문서의
후속 gate로 남긴다.

---

## TS-2026-08-09-58 — Dart analysis_options를 실제 semantic unit boundary로 소유한다

### 증상

linter provider가 `test_data/rules/experiments/nnbd/analysis_options.yaml`을 읽었지만 AnalysisPlan에는 없는
설정이라 canonical migration이 전체 실행을 거부했다.

### 근본 원인

planner는 analysis_options를 context artifact로 알면서도 pubspec/package_config만 unit root로 만들었다.
그러나 nested analyzer option은 language experiment와 resolution semantics를 바꾼다.

### 적용한 수정과 검증

nested `analysis_options.yaml`도 Dart analysis-unit root로 승격했다. 전용 planner test와 linter 592-file real
run에서 12 units가 provider execution context와 exact 일치했고 cold/warm semantic·bundle digest가 같았다.

---

## TS-2026-08-09-59 — 완료된 빈 결과와 provider 실패를 cache에서 구분한다

### 증상

Dart의 의미 사실 0개 unit은 cache writer가 저장했지만 reader가 `documents.is_empty()`만 보고 버려 매 warm
run마다 language server를 다시 시작했다.

### 근본 원인과 수정

빈 결과를 전부 실패로 취급했다. 이제 `EmptySemantic` 완료 diagnostic이 있는 정확한 빈 결과만 재사용하고,
timeout/stopped/일반 실패의 빈 결과는 계속 재실행한다. 전용 cache round-trip test로 고정했다.

---

## TS-2026-08-09-60 — 전체 executable hash로 모든 언어 cache를 무효화하지 않는다

### 증상

provider normalization cache는 안정적이어도 unrelated Rust 코드 rebuild 뒤 source manifest가 “이전 없음”으로
판정되어 모든 language unit이 affected source range로 강제 재실행됐다.

### 근본 원인과 수정

source dependency, framework, architecture cache key가 whole executable hash를 포함해 명시적 schema version을
무력화했다. 각각의 contract version을 cache authority로 삼고 executable hash를 제거했다. census/planner,
framework, projection 의미가 변할 때 해당 version을 의도적으로 올리는 규칙을 적용한다.

---

## TS-2026-08-09-61 — C# 표현식 본문 호출을 클래스가 아니라 실제 메서드에 붙인다

### 증상

scip-dotnet 원본에는 `DbContext.cs:597`의 `SaveChanges(bool)` 호출이 있었지만 canonical graph에서는
`DbContext.SaveChanges()` 발신 간선이 없었다. 호출은 `DbContext` 클래스 source로 들어와 TracePath가 끊겼다.

### 근본 원인

scip-dotnet occurrence의 `enclosing_range`가 비어 있었고 기존 fallback은 중괄호 scope만 사용했다.
`public int SaveChanges() => SaveChanges(true)` 같은 표현식 본문에는 메서드 중괄호가 없어 클래스가 소유자가
됐다.

### 적용한 수정과 검증

C# CST에서 method/constructor/operator/local-function/property 등 가장 가까운 실행 소유자의 **이름 범위**를
call site와 함께 수집하고, SCIP definition range index로 exact symbol을 찾는다. 이름이나 경로 유사도로
추정하지 않는다. EF Core v10.0.10에서 다음을 원본과 canonical SQLite evidence로 대조했다.

- 0-based 596: `DbContext.SaveChanges()` → `DbContext.SaveChanges(bool)`
- 0-based 647: `DbContext.SaveChanges(bool)` → `IStateManager.SaveChanges()`
- 비동기 overload도 0-based 731, 789에 정확히 연결

모든 결과는 `confirmed`, `call_site`, scip-dotnet 0.2.14 producer와 exact source span을 가진다.

---

## TS-2026-08-09-62 — C# generic base에서 type argument를 상속 대상으로 고르지 않는다

### 증상

원본 `class DbContext : IInfrastructure<IServiceProvider>, ...`의 provider 관계는 정확했지만 최종 graph에는
`IInfrastructure` implements 하나만 빠졌다.

### 근본 원인

공통 `type_name_leaf`가 C# tree-sitter의 `type_argument_list`를 generic argument container로 알지 못해 바깥
`IInfrastructure` 대신 마지막 안쪽 타입 `IServiceProvider`를 source syntax target으로 기록했다. provider
target과 독립 syntax evidence가 불일치해 관계를 버린 것은 올바른 abstention이었지만 syntax inventory가
틀렸다.

### 적용한 수정과 검증

`type_argument_list` 내부를 바깥 타입 이름 후보에서 제외하고 generic-base 회귀 테스트를 추가했다. 보정 후
`DbContext`의 원본 interface 4개가 모두 confirmed implements이며 warm 2회 digest가 동일하다.

---

## TS-2026-08-09-63 — 대형 provider 후처리의 O(N²) 관계 병합과 AST 조상 재탐색을 제거한다

### 증상

provider cache가 있어도 EF Core 5,536 C# 파일 재분석이 약 9분 걸렸고, 첫 정확도 보정 cold는 978.627초였다.
scip-dotnet이 끝난 뒤에도 수분 동안 진행률이 멈춰 보였다.

### 근본 원인

1. `dedupe_provider_relations`가 relation마다 전체 누적 relation을 다시 훑었다. 25만 관계에서 O(N²)이었다.
2. definition/type-use inventory가 AST 노드마다 실행 본문 여부를 알기 위해 모든 조상을 여러 번 재탐색했다.
3. type-use owner 선택도 파일의 모든 definition을 반복 검색했다.

### 적용한 수정

- relation identity `(from,to,kind,path)`별 insertion-order index만 조회한다. 겹친 CALLS 중 더 짧은 범위를
  선택하는 legacy first-match 규칙은 그대로이며 새 구현과 legacy reference 결과를 테스트로 비교한다.
- AST 재귀 순회가 `inside_executable` 상태를 자식에게 전달한다. Python class block 예외 등 언어별 규칙은
  동일하다.
- definition name range index로 현재 조상 범위 안 후보만 조회한다.
- direct Language IR와 canonical linker에 opt-in phase timing을 넣어 감이 아니라 실측으로 최적화한다.

### 검증 결과

- provider batch merge: 수분대 → 2.1초
- definition inventory: 80.862초 → 5.953초
- type-use inventory: 27.621초 → 3.981초
- source inventory: 128.133초 → 28.533초
- 전체 warm: 266.992초 → 162.931초
- 최적화 전후 최종 두 run의 semantic `56b3fb...f2f`, bundle `d8eb9b...9d6`, counts가 동일

남은 C# cold 병목은 scip-dotnet+SCIP 변환 369.212초와 canonical link 80.987초다. 최종 코드의 빈 캐시
cold 측정과 converter/canonical의 별도 프로파일링을 후속 gate로 둔다.

---

## TS-2026-08-09-64 — Java 격리 workspace의 빌드 지원 파일 누락과 고정 1GB OOM을 분리한다

### 증상

Spring Framework 8,982 Java 파일 cold 실행에서 build-backed JDTLS가 66초 뒤 semantic fact 0개를 반환했고,
source-only fallback은 진행률 35%에서 14분 이상 CPU를 사용했다. 프로세스 로그에는
`OutOfMemoryError: Java heap space`가 남았지만 JVM은 계속 살아 있었다.

### 근본 원인

서로 다른 두 문제였다.

1. writable provider workspace가 SourceManifest의 `Included` 파일만 복사했다. Gradle buildSrc가 읽는
   `buildSrc/config/checkstyle/checkstyle.xml`은 분석 대상이 아닌 `Unsupported` 파일이어서 복사되지 않았다.
   실제 JDTLS log에서 Checkstyle 설정 파일 부재로 Gradle project import가 실패한 것을 확인했다.
2. Windows JDTLS launcher가 저장소 크기와 시스템 여유 메모리에 관계없이 `-Xmx1G`를 사용했다. build import
   실패 후 8,982개 문서를 여는 source-only fallback이 diagnostics reconcile을 수행하다 heap OOM이 났다.

### 적용한 수정

- provider 격리 workspace는 분석에 포함된 파일 외에도 regular·non-sensitive `Unsupported` 파일을 별도
  support input으로 seal/digest/copy한다. 이 파일들은 복사됐다는 이유만으로 source fact가 되지 않는다.
- JDTLS heap은 예정 Java 파일 수와 실행 시점 provider memory budget으로 계산한다. Spring workload와
  6,546MB budget에서는 4,018MB이며 전체 budget의 75%를 넘지 않는다.
- JVM에 `ExitOnOutOfMemoryError`를 적용해 OOM 이후 CPU만 쓰는 zombie provider를 막는다.
- 제품이 소비하지 않는 editor compiler diagnostics는 `java.diagnostic.filter=**/*.java`로 차단한다.
  호출·정의·상속 LSP 질의와 source evidence는 그대로 유지한다.
- 기존 build-support 누락/1GB 결과가 cache에 남지 않도록 Java normalization marker를 올렸다.

### 현재 검증

workspace support-copy, Java settings, adaptive heap 회귀 테스트와 Python launcher 문법 검증은 통과했다.
Spring 완전 빈 cache cold/warm, exact digest, 실제 source call/type 대조는 같은 corpus 재실행 후 이 항목에
수치로 추가한다. 이 재측정 전에는 Java가 합격했다고 쓰지 않는다.

### 재발 시 점검 순서

1. `java-v2/.metadata/.log`에서 build import 실패의 최초 원인을 본다.
2. 원본 빌드가 참조한 파일이 격리 workspace에도 같은 digest로 존재하는지 확인한다.
3. JVM command의 계산된 heap과 `.metadata/.log`의 OOM을 확인한다.
4. build-backed 결과가 비었을 때만 source-only fallback이 시작됐는지 확인한다.
5. 최종 canonical 관계를 실제 Java source 위치와 대조한다.

---

## TS-2026-08-09-65 — Gradle toolchain 발견과 JDTLS system library 등록은 별도 계약이다

### 증상

Spring Framework는 Gradle 실행 JVM과 별도로 Java 25 toolchain을 요구한다. 관리 JDK 25 경로를
`java.import.gradle.jvmArguments`의 `-Dorg.gradle.java.installations.paths=...`로 보냈지만 Gradle은 이를
toolchain 경로로 쓰지 않았고, Gradle 쪽을 고친 뒤에도 JDTLS는 `JavaSE-25` system library가 없다며 대량의
오류를 냈다.

### 근본 원인

서로 다른 두 설정을 하나로 취급했다.

1. `org.gradle.java.installations.paths`는 Gradle project property이므로 import argument에
   `-Porg.gradle.java.installations.paths=...`로 전달해야 한다.
2. Gradle이 JDK를 찾는 것과 JDTLS execution environment가 해당 JDK를 아는 것은 별개다. JDTLS에는
   `java.configuration.runtimes`의 `JavaSE-25` 항목도 필요하다.

### 적용한 수정과 검증

- 관리 toolchain 경로를 Gradle `-P` argument로 전달한다.
- 각 JDK의 `release` 파일에서 실제 major version을 읽어 `JavaSE-1.8` 또는 `JavaSE-{major}` runtime을
  생성하고, 가장 높은 버전을 default로 등록한다.
- Spring JDTLS log에서 JDK 25가 `JavaSE-25`로 등록됐고 `Missing system library`가 0임을 확인했다.
- 관리 toolchain 경로가 바뀌면 provider cache가 무효화되도록
  `CODE_MEMORY_JAVA_TOOLCHAIN_PATHS`를 project config digest에 포함했다.

### 남은 gate

Spring cold/warm canonical digest와 실제 source 관계 대조가 끝날 때까지 Java 전체 합격으로 보지 않는다.

---

## TS-2026-08-09-66 — 대형 Java의 member implementation 한 건이 전체 타입 계층을 다시 계산한다

### 증상

Spring 8,982 Java 파일의 build-backed JDTLS가 정상 초기화된 뒤에도 진행률 35%에서 15분 이상 머물렀다.
JVM은 살아 있었고 CPU를 계속 사용했지만 결과는 없었다.

### 근본 원인

명시적 상속 target 이름을 가진 모든 type의 method에 `textDocument/implementation`을 호출했다. JDTLS
스레드 덤프에서 단 한 요청이 `ImplementationsHandler → ImplementationCollector → TypeHierarchy`를 타고
Spring 전체 타입 계층을 다시 만들며 10분 넘게 실행 중인 것을 확인했다. request timeout 중간에 progress
notification이 오면 client receive timeout이 갱신되어 session deadline까지 계속될 수 있었다.

### 적용한 수정

- 대형 Java workspace에서는 provider-wide member implementation 검색을 호출하지 않는다.
- 이미 provider가 확정한 local `extends/implements` type pair 안에서, 양쪽의 provider method signature가
  정확히 같고 자식 source에 실제 `@Override`가 명시된 경우만 override를 만든다.
- 주석 속 `@Override`, unannotated override, signature가 다른 overload는 관계를 만들지 않는다.
- 누락 가능성은 `PartialCoverage` diagnostic으로 남기며 type-level hierarchy와 call 관계에는 영향을 주지
  않는다.

### 검증 상태

annotation positive/negative/comment 회귀 테스트와 Java 관련 26개 unit test는 통과했다. 첫 Spring
full-budget run은 이 결함을 재현한 invalid run으로 중단했으며, 보정 release의 완전 빈 cache cold/warm 및
실제 source 대조가 완료돼야 이 항목을 닫는다.

---

## TS-2026-08-09-67 — LSP 배치의 응답 없는 한 항목이 완료된 형제 결과와 후속 배치를 막지 않는다

### 증상

Spring build-backed JDTLS가 심볼 인덱스를 만든 뒤 `src/main/java24`, `src/test/java21`처럼 현재 project build
path 밖인 source에 정의 질의를 보냈다. 대부분은 오류 응답을 돌려줬지만 일부 요청은 끝내 응답하지 않았다.
기존 client는 배치의 마지막 한 응답을 기다리느라 이미 받은 정상 응답도 반환하지 못했고, 같은 문서의 다음
심볼마다 60~300초 timeout을 반복했다.

### 근본 원인

1. `request_batch_inner`가 `receive()` 오류 하나를 배치 전체 오류로 승격해 부분 완료 결과를 버렸다.
2. progress/diagnostic notification마다 새 `receive()`를 호출해 응답 대기 시간이 사실상 연장될 수 있었다.
3. 한 문서의 같은 요청 종류가 이미 응답 불능임을 확인해도 뒤의 심볼에 동일한 질의를 반복했다.

### 적용한 수정

- 배치 시작 시 절대 response deadline을 만들고 notification이 와도 연장하지 않는다.
- timeout 시 이미 완료된 response는 입력 순서 그대로 보존하고, 아직 pending인 항목만 error로 만든다.
- pending request에는 `$/cancelRequest`를 best-effort로 보낸다.
- timeout scope를 `LSP method + document URI`로 격리한다. 같은 문서의 같은 요청 종류는 그 session에서 다시
  기다리지 않고 미해결로 남기지만, 다른 요청 종류와 다른 문서는 계속 분석한다.
- session timeout·stdout 종료 같은 연결 단위 실패는 여전히 전체 provider 실패로 처리한다.

### 검증 상태

부분 완료 응답 보존·결정적 취소 순서·method/document 격리 회귀 테스트가 통과했다. 첫 보정 run은 부분 결과
보존만으로 같은 build-path 밖 문서에 timeout을 반복하는 두 번째 병목을 드러내 판정용에서 제외했다. 격리까지
포함한 fresh Spring cold/warm과 실제 source 대조가 완료돼야 이 항목을 닫는다.

---

## TS-2026-08-09-68 — 대형 workspace의 사전 질의 한도를 결과 조립 단계에서도 강제한다

### 증상

Spring 8,982 Java 파일에서 JDTLS 초기화·문서 심볼 수집·실제 LSP 요청 wall time의 합은 약 5분이었지만,
`enrichment` 단계는 1,717초를 사용해 전체 1,800초 session limit에 도달했다. 성능 로그에는
`textDocument/definition` 18,117건만 기록됐는데도 요청 wall time 밖에서 약 24분이 사라졌다.

### 근본 원인

대형 workspace planner는 상속 target 전체와 type-use 최대 2,048개만 사전 질의하도록 제한했지만, 결과 조립
루프는 그 선택 집합을 알지 못했다. 사전 질의되지 않은 나머지 type-use 위치마다 `definitions_at`을 다시
호출했고, 빈 응답에는 최대 3회 직렬 재시도와 대기가 발생했다. 즉 계획 단계의 제한이 실행 단계에서
무효화됐다.

### 적용한 수정

- 실제로 사전 질의한 `(document URI, line, UTF-16 character)` 집합을 명시적으로 보존한다.
- 대형 workspace 결과 조립은 그 집합에 포함된 위치만 definition cache에서 해석한다.
- 작은 workspace는 기존 전체 질의 동작을 유지한다.
- 성능 receipt에 `largeDefinitionSelected`를 기록해 계획과 실제 요청을 대조할 수 있게 했다.
- 선택되지 않은 위치가 결과 조립 중 새 LSP 요청으로 승격될 수 없음을 회귀 테스트로 고정했다.

### 검증 상태

전용 LSP 테스트 27개가 통과했다. Spring 보정 release의 완전 빈 cache cold/warm에서 기존 계획 범위의 사실
ID와 semantic digest, 실제 source 관계를 대조한 뒤 닫는다. 30분 timeout 결과는 부분 결과이므로 최종 Java
정확도·성능 기준선으로 사용하지 않는다.

---

## TS-2026-08-09-69 — LSP 정의의 전체 선언 범위를 가장 작은 중첩 심볼로 바꾸지 않는다

### 증상

Spring direct-call shadow에서 JDTLS가 확정한 48,945개 호출·생성 관계를 실제 source token과 전수 대조했다.
157개(0.32%)는 `hasText → hasLength`, `doConvertFromMessage → getMessageConverter`처럼 호출 위치의 이름과
저장된 target 이름이 달랐다. 관계에는 confidence 1.0과 exact evidence가 붙어 있어 그대로 두면 명백한
거짓 confirmed 간선이 된다.

### 근본 원인

`textDocument/definition`은 서버와 위치에 따라 `targetSelectionRange` 또는 메서드 전체 선언 범위를
돌려준다. 기존 `find_lsp_symbol_at_range`는 반환 범위 안에 selection point가 있는 모든 심볼 중 **자기
선언 범위가 가장 작은 심볼**을 골랐다. 따라서 메서드 전체 범위를 받으면 올바른 메서드보다 그 본문 안의
작은 중첩 심볼을 target으로 선택할 수 있었다. 배치 응답 순서나 JDTLS 오답이 아니라 client-side range
정규화 결함이었다.

### 적용한 수정

target 해석 순서를 다음의 닫힌 규칙으로 바꿨다.

1. target 시작 위치와 symbol selection 시작 위치가 정확히 같은 심볼
2. target 전체 범위와 symbol 선언 전체 범위가 정확히 같은 심볼
3. target을 포함하는 가장 작은 선언
4. 호환 fallback으로 target 안에 selection이 여러 개면 가장 큰 선언

메서드 전체 범위 안에 더 작은 중첩 메서드가 있어도 정확한 외부 메서드를 고르는 회귀 테스트와
`targetSelectionRange`가 같은 행의 정확한 심볼을 고르는 테스트를 추가했다. 이 정규화는 모든 LSP 언어의
provider cache 의미를 바꾸므로 공통 language cache contract를 v153으로 올렸다.

### 검증 상태

Java/LSP 전용 테스트 33개는 통과했다. 첫 direct-call cold는 결함을 발견하기 위한 invalid shadow이며 최종
기준선으로 쓰지 않는다. 보정 fresh Spring cold에서 source-token mismatch 0, confirmed-without-evidence 0,
dangling endpoint 0과 cold/warm digest 일치를 확인해야 닫는다.

---

## TS-2026-08-09-70 — 논리적 무제한 표식을 실제 메모리 예약 크기로 사용하지 않는다

### 증상

Spring 보정 cold run에서 JDTLS 실행 직후 Rust worker thread가 `capacity overflow`로 panic했다. Java provider는
126초 만에 `indexer-failed`가 되었지만 상위 파이프라인은 JavaScript/Python 보조 파일의 사실만으로 canonical
bundle을 만들고 exit code 0을 반환했다. 따라서 해당 bundle의 Java 정의는 0개였고 판정 자료로 사용할 수 없다.

### 근본 원인

대형 Java 호출 후보를 파일별로 공정하게 정렬하는 함수는 `usize::MAX`를 “모든 후보를 정렬한 뒤 상위
scheduler가 실제 공용 request budget을 나눈다”는 **논리적 무제한 표식**으로 받는다. 그런데 함수가
`Vec::reserve(limit - selected.len())`를 호출해 이 표식을 실제 allocation size로 해석했다. 분석량이 많아서
메모리가 부족한 문제가 아니라, 약 50만 후보를 처리하면서 사실상 `usize::MAX` 크기의 메모리를 한 번에
예약하려 한 산술/계약 결함이었다.

### 적용한 수정

- sentinel 기반 사전 예약을 제거하고 실제 push 수만큼 정상적으로 증가하게 했다.
- `usize::MAX`와 여러 파일 후보를 직접 넣어 panic 없이 전체 순서를 반환하는 회귀 테스트를 추가했다.
- LSP 전용 테스트 34개가 통과했다.

### 검증 상태

panic을 낸 v2 cold bundle은 전체 폐기했다. 수정 release와 완전히 새로운 cache를 사용한 v3 Spring cold는
Java provider 및 canonical bundle 생성까지 panic 재발 없이 완료되어 이 결함은 닫혔다. v3은 별도의 LSP
selection 불변식 결함으로 최종 기준선에서 제외한다. 핵심 scheduled provider 실패를 canonical publish
성공처럼 보이지 않게 하는 제품 상태/검문 계약은 후속 확인한다.

---

## TS-2026-08-09-71 — LSP 심볼의 이름 위치는 자기 선언 범위 안에 있어야 한다

### 증상

Spring v3 cold의 provider 호출·생성 관계 51,990개를 실제 source token과 전수 대조했더니 51,989개가
일치하고 1개가 불일치했다. `InterfaceMaker.java`의 `new ClassEmitter(v)`가
`ClassEmitter.setTarget()`을 생성하는 관계로 기록됐다.

### 근본 원인

JDTLS document symbol에서 `setTarget`의 실제 선언 범위는 47~55행인데 selection/name 위치가 비정상적으로
`0:0`이었다. 같은 위치로 반환된 불완전한 definition target을 client가 “selection 시작 exact match”로
최우선 선택하면서, 존재할 수 없는 provider 좌표가 확정 관계가 됐다. 호출 토큰이나 이름 유사도로 생긴 문제가
아니라 provider symbol의 내부 불변식을 검증하지 않은 것이 원인이다.

### 적용한 수정

- LSP symbol target 후보는 selection point가 자기 full declaration range 안에 있을 때만 사용한다.
- 불변식을 어긴 심볼은 같은 파일의 이름으로 추측 복구하지 않고 unresolved로 남긴다.
- 실제 `setTarget` 형태의 malformed symbol이 target으로 선택되지 않는 회귀 테스트를 추가했다.
- 모든 LSP 언어의 정규화 의미가 바뀌므로 language cache contract를 v154로 올렸다.

### 검증 상태

LSP 전용 테스트 35개가 통과했다. v3은 99.998077% 일치였지만 확정 오답 1개가 있으므로 invalid shadow로
분류한다. fresh v4 cold에서 호출·생성 source-token mismatch 0, cold/warm exact digest, 대표 TracePath를 다시
검증해야 닫는다.

---

## TS-2026-08-09-72 — LSP 표시 이름 전체 길이를 정의 이름의 source range로 쓰지 않는다

### 증상

Spring v3 canonical graph에서 `DispatcherServlet.doDispatch()`는 존재했지만 실제 source에 있는
`doDispatch -> processDispatchResult`와 `doDispatch -> triggerAfterCompletion` 호출이 사라졌다. raw JDTLS
provider에는 두 target 정의와 호출 occurrence가 모두 있었다.

### 근본 원인

JDTLS method label은 `processDispatchResult(HttpServletRequest, HttpServletResponse, ...)`처럼 매개변수까지
포함한다. provider 변환기는 정의 시작 위치에 이 **표시 label 전체 UTF-16 길이**를 더해 definition evidence
range를 만들었다. 긴 시그니처는 실제 첫 source line 끝을 넘어갔고 `SourceCoordinates` 검증에서 정의가
탈락했다. 그 정의를 endpoint로 쓰는 실제 호출도 canonical IR에서 함께 빠졌다.

### 적용한 수정

- Java callable/type 정의 evidence의 끝은 protocol 표시 label이 아니라 실제 source name token
  (`processDispatchResult`, `ClassEmitter`) 길이로 계산한다.
- provider selection이 자기 선언 밖이면 선언 범위 안에서 정확한 source name이 하나일 때만 selection을
  복구한다. 0개 또는 여러 개면 추측하지 않고 심볼을 제외하고 partial-coverage로 기록한다.
- 정상 긴 label, 고유 복구, 재귀 호출 때문에 복구 후보가 여러 개인 abstention을 회귀 테스트로 고정했다.
- Java provider cache에 `java-definition-name-evidence.v1` marker를 추가했다.

### 검증 상태

LSP 전용 테스트 37개가 통과했다. 이 원인을 확인한 시점에 v4 cold는 판정 가치가 없어 중단했다. v5의
callable/type 정의 111,097개는 source range와 name token이 전부 일치해 label-length 결함 자체는 닫혔다.
다만 v5는 별도의 malformed declaration-end 결함으로 핵심 심볼을 제외했으므로 최종 Java 기준선은 아니다.

---

## TS-2026-08-09-73 — Java 선언 끝 좌표만 깨졌을 때 정확한 source selection까지 버리지 않는다

### 증상

Spring v5는 confirmed 호출의 오답을 막았지만 performance receipt에
`rejectedMalformedJavaSymbols=2524`를 기록했다. `DispatcherServlet.doDispatch`, 내부 타입
`HeadersState`, `HeadersPredicate`가 provider graph에서 사라졌고, 이에 속한 실제 호출도 함께 누락됐다.

### 영향

거짓 관계는 없지만 대형 Java 프로젝트의 정상 정의와 실행 흐름 일부가 사라진다. 단순 정확도 표본은 통과해도
사용자가 보는 구조 지도와 TracePath는 끊길 수 있다.

### 잘못 짚기 쉬운 원인

JDTLS가 selection/name 위치까지 전부 틀린 것이 아니다. v3 원본 정의를 전수 검사한 결과 선언 밖
callable/type selection 2,787개 중 2,780개는 실제 선언 이름 token을 정확히 가리켰다. 다수는 선언 시작과
selection은 정상이고 declaration end만 `0:0`으로 역전된 형태였다.

### 근본 원인

기존 불변식은 selection이 full declaration range 안에 있어야만 심볼을 유지했다. declaration end 자체가
역전되거나 source 밖인 경우에도 동일 규칙을 적용해, 정확한 provider selection까지 잘못 거부했다.

### 적용한 수정

- declaration range가 실제 source 좌표로 유효한지 시작·끝 순서와 UTF-16 column으로 검증한다.
- range가 유효하지 않을 때만, selection이 선언 시작 이후이고 source의 정확한 Java identifier가 provider
  base name과 같으면 declaration end를 그 name token까지 복구한다.
- 유효한 선언 범위 밖 selection은 선언 안에서 이름이 정확히 한 번 발견될 때만 기존처럼 이동한다.
- 정상 선언 밖의 use-site, `0:0`의 잘못된 selection, 복수 후보는 confirmed 정의로 승격하지 않는다.
- Java provider cache marker를 `java-definition-name-evidence.v2`로 올렸다.

### 검증 결과

malformed end 복구, 정상 range 밖 use-site의 고유 선언 복구, 복수 후보 abstention, `setTarget@0:0` 거부를
포함한 LSP 테스트 39개가 통과했다. Spring v6에서 repaired 3,451 / rejected 215로 보정됐고,
CALLS/CONSTRUCTS 51,982개와 callable/type definition 113,406개를 원본 source에 전수 대조해 range·target
불일치 0을 확인했다. `DispatcherServlet` 대표 TracePath도 복구됐으며 cold 639.897초 / warm 122.043초의
semantic·bundle digest가 정확히 같았다.

### 재발 시 점검 순서

1. performance receipt의 repaired/rejected Java symbol 수를 확인한다.
2. 제외된 symbol selection의 source token과 declaration start/end를 각각 대조한다.
3. 정확한 selection이더라도 declaration range가 유효하면 임의 축소하지 않는다.
4. raw CALLS/CONSTRUCTS target token 전수 검증과 대표 TracePath를 함께 본다.

### 남은 한계 또는 후속 gate

JDTLS가 선언 시작·끝·selection을 모두 틀리거나 source name 후보가 여러 개이면 복구하지 않는다. 이는 가짜
confirmed 관계보다 typed gap을 선택하는 의도된 한계다.

---

## TS-2026-08-09-74 — 저장소의 모든 `.cs` 파일은 C# 컴파일 입력이 아니다

### 증상

EF Core v10.0.10에서 source census는 C# 5,534개를 열거했지만 scip-dotnet 결과에는 5,174개만 있었다.
기존 coverage는 나머지 360개를 `provider_execution_incomplete`로 표시해 실제 provider 실패처럼 보였다.

### 영향

사용자에게 존재하지 않는 분석 실패 360개를 보고하며, 언어 coverage와 신뢰 상태를 부당하게 낮춘다.
반대로 이 숫자를 무조건 무시하면 실제 provider 누락도 숨길 수 있다.

### 잘못 짚기 쉬운 원인

scip-dotnet가 큰 프로젝트에서 임의로 파일을 자른 것이 아니다. 누락 360개는 정확히 다음 네 경로뿐이었다.

- `EFCore.SqlServer.FunctionalTests/Scaffolding/Baselines` 113개
- `EFCore.InMemory.FunctionalTests/Scaffolding/Baselines` 112개
- `EFCore.Sqlite.FunctionalTests/Scaffolding/Baselines` 88개
- `EFCore.Cosmos.FunctionalTests/Scaffolding/Baselines` 47개

각 프로젝트는 동일 경로를 `<Compile Remove="Scaffolding\Baselines\**\*" />`로 제거하고 `None` item으로
포함한다. 즉 저장소 artifact이지만 compiler input은 아니다.

### 근본 원인

AnalysisPlan은 확장자 기준 source census와 compiler의 active source set을 구분했지만, C# scheduler가
MSBuild의 명시적 제외를 소비하지 않았다. merge 단계는 제외 파일의 정확한 identity 대신 개수만 알아
모든 비반환 파일에 generic provider failure 사유를 붙였다.

### 적용한 수정

- literal이고 unconditional인 `<Compile Remove>` glob만 project root 기준으로 해석한다.
- `Condition`, `$(...)`, `@(...)`, `%(...)`가 포함된 동적 규칙은 추측하지 않는다.
- scheduler receipt에 정확한 `project-config-excluded` 파일을 남기고 provider input에서 제외한다.
- 최종 coverage에서 동일 active-set을 다시 대조해 `excluded + missing_compile_context`로 기록한다.
- C# fixture에서 build 제외와 provider 실패 사유가 섞이지 않는 회귀 테스트를 추가했다.

### 검증 결과

최종 fresh cold와 warm 모두 planned 5,534 / scheduled active 5,174 / project excluded 360으로 동일했다.
SQLite coverage는 C# `indexed` 5,174, `excluded` 360이며 360개 gap은 모두
`missing_compile_context`다. `provider_execution_incomplete`는 0이다.

### 재발 시 점검 순서

1. source census 파일 집합과 provider 고유 document path를 diff한다.
2. 누락 파일의 가장 가까운 project와 `Compile Remove/Include`를 대조한다.
3. 조건식이나 MSBuild property가 있으면 정적 추측으로 제외하지 않는다.
4. excluded와 missing/provider-failed를 file identity 기준으로 별도 집계한다.

### 남은 한계 또는 후속 gate

조건부 item, custom MSBuild target, 여러 project가 공유하는 linked source의 완전한 active set은 literal XML
해석만으로 확정할 수 없다. 이런 경우는 provider 결과와 실제 MSBuild evaluation receipt가 없으면 partial로
남겨야 하며, 다른 project가 compile하는 파일을 한 project의 remove만 보고 전역 제외해서는 안 된다.

---

## TS-2026-08-09-75 — scip-dotnet의 Roslyn UTF-16 column을 SCIP UTF-8 column으로 정규화한다

### 증상

`ModelBuilderTest.NonRelationship.cs`의 `protected class Entityß` 정의가 scip-dotnet range
`[2231, 24, 31]`로 들어왔다. source의 UTF-8 byte 기준 정확한 끝은 32라서 기존 canonical evidence는
멀티바이트 문자의 중간에서 끝났다.

### 영향

ASCII 저장소에서는 숨지만 국제 문자·이모지 identifier가 있는 C# 소스에서 정의/호출 근거가 invalid가 되거나
잘못된 token으로 잘릴 수 있다. 이후 exact range join과 TracePath endpoint도 함께 유실될 수 있다.

### 근본 원인

scip-dotnet 0.2.14는 Roslyn `TextSpan`의 UTF-16 code-unit column을 그대로 내보낸다. 우리 SCIP 경계는
portable source coordinate를 UTF-8 byte column으로 해석했지만 provider별 coordinate normalization이 없었다.

### 적용한 수정

- C# SCIP document를 읽는 즉시 occurrence `range`와 `enclosing_range`를 source line 기준
  UTF-16 code unit → UTF-8 byte column으로 변환한다.
- 변환 불가능한 좌표는 임의 보정하지 않고 기존 typed-gap 검증으로 넘긴다.
- `Entityß`, supplementary-plane 이모지, invalid column 회귀 테스트를 추가했다.
- C# provider normalization cache marker를 `csharp-exact-call-owner.v2`로 올렸다.

### 검증 결과

최종 raw cache의 occurrence 1,761,195개, global definition 215,986개, range가 있는 relation 467,927개를
원문 바이트와 전수 대조했다. out-of-range, invalid UTF-8 boundary, 빈 global definition, 빈 relation range는
모두 0이다. `Entityß`는 `[2231, 24, 32]`로 보정됐다. canonical source evidence 322,032개도 content
digest·line·UTF-8 column·byte offset 불일치가 모두 0이다.

### 재발 시 점검 순서

1. non-ASCII 문자가 occurrence 앞과 identifier 안에 있는 fixture를 사용한다.
2. provider raw column과 source의 UTF-16/UTF-8 길이를 각각 계산한다.
3. 변환은 provider 경계에서 한 번만 수행하고 canonical 내부 좌표 단위를 섞지 않는다.
4. raw occurrence뿐 아니라 최종 SQLite evidence의 absolute byte offset까지 검증한다.

### 남은 한계 또는 후속 gate

향후 scip-dotnet가 SCIP 규약에 맞춰 UTF-8 column을 직접 내보내면 이 보정은 이중 변환이 된다. provider
버전/coordinate contract를 cache identity와 함께 고정하고 업그레이드 시 non-ASCII fixture를 먼저 실행한다.

---

## TS-2026-08-09-76 — JavaScript HTTP client 호출을 서버 route로 승격하지 않는다

### 증상

NestJS 저장소에서 Express endpoint 279개, Fastify endpoint 24개가 검출됐지만 대부분
`request(app.getHttpServer()).get('/test')`, `request(server).post('/graphql')` 같은 Supertest 요청이었다.
화면에는 존재하지 않는 Express/Fastify API가 수백 개 생길 수 있었다.

### 영향

테스트가 호출한 URL을 서버가 이 파일에서 등록한 endpoint로 거짓 표현한다. 프레임워크 경계, API 수,
TracePath 시작점이 모두 오염된다.

### 잘못 짚기 쉬운 원인

프로젝트가 Express adapter를 포함한다는 사실이나 receiver 이름이 `app`/`server`라는 사실만으로는 등록을
증명하지 못한다. `request(server).get(...)`에서 `.get` 직전의 마지막 identifier를 단순 추출하면 호출식의
인자 `server`를 receiver로 오인한다.

### 근본 원인

registration parser가 `.get/.post` 앞의 전체 표현식 형태를 보지 않고 마지막 identifier와 관습적 이름만
검사했다. 또한 `.route()` chain은 ownership 검사보다 먼저 확장됐다.

### 적용한 수정

- `.get` 직전 표현식이 `)`, `]`, `}`로 끝나는 call/index expression이면 서버 등록으로 인정하지 않는다.
- 실제 `express()`/`express.Router()`/`Fastify()` 생성 또는 해당 framework의 exact import가 있는 bare
  receiver만 등록으로 인정한다.
- `.route()` chain도 같은 ownership gate를 먼저 통과한다.
- Supertest client 3종과 실제 Express/Fastify 등록을 한 테스트에 넣고 framework 회귀 72개를 통과시켰다.

### 검증 결과

동일 NestJS 원본에서 Express endpoint 279→1, Fastify endpoint 24→1로 줄었다. 남은 둘은
`tools/benchmarks/src/frameworks/{express,fastify}.ts`의 실제 `GET /` 등록이다. NestJS endpoint 280개와
대표 `POST/GET /cats` handler 연결은 유지됐다.

### 재발 시 점검 순서

1. route 표본의 원본 줄이 registration인지 HTTP client invocation인지 확인한다.
2. receiver의 이름이 아니라 생성/import 근거를 확인한다.
3. framework별 endpoint를 source scope(test/runtime)와 handler resolution별로 집계한다.
4. 수정 후 실제 route 유지와 client route 제거를 동시에 검증한다.

### 남은 한계 또는 후속 gate

framework instance가 다른 파일에서 매개변수로 주입되는 고급 registration helper는 import/생성 근거가 없으면
abstain할 수 있다. 이름만 보고 연결하는 것보다 typed gap을 유지한다.

---

## TS-2026-08-09-77 — zero-width provider document sentinel은 코드 정의가 아니다

### 증상

scip-typescript는 파일마다 `0:0-0:0` document sentinel occurrence를 낸다. 대부분은 reconciliation에서
제외됐지만 두 파일은 provider가 Namespace로 분류해 표시명 `ts\``인 namespace node와 길이 0
source-definition evidence가 최종 SQLite에 남았다.

### 영향

실제 코드가 아닌 파일 sentinel이 영역 구성원이나 관계 endpoint가 되며, “모든 confirmed 정의는 원문 범위를
가진다”는 계약을 깨뜨린다.

### 근본 원인

adapter가 provider kind만 canonical kind로 바꾸고 source span이 비어 있는지는 확인하지 않았다. 단순히
정의를 일찍 버리면 sentinel을 향한 provider relation 수천 개가 일반 unresolved target으로 바뀌는 2차 문제도
발생했다.

### 적용한 수정

- start/end byte offset이 같은 provider definition은 canonical definition으로 승격하지 않는다.
- 해당 provider symbol ID는 `ignored/discarded` 집합에 보존해 이를 향하는 relation도 일반 unresolved 사실로
  오인하지 않는다.
- sentinel이 Namespace라고 주장하고 실제 CALLS relation target인 fixture로 node/edge 모두 제거됨을 고정했다.

### 검증 결과

NestJS 최종 source evidence zero-length 2→0, 잘못된 namespace 2→0이다. 잘못된 relation을 무작정 gap으로
바꿨던 중간 run(gaps 9,240)은 폐기했고, 최종은 nodes 6,907 / edges 18,207 / evidence 20,019 /
gaps 2,202로 두 번 반복해 semantic·bundle digest가 동일했다.

### 남은 한계 또는 후속 gate

raw provider artifact의 document sentinel 자체는 provider fidelity를 위해 남는다. raw audit에서는 이것을 실제
빈 정의와 구분해야 하며, canonical에 승격됐는지만 release gate로 본다.

---

## TS-2026-08-09-78 — scip-typescript non-BMP column은 UTF-16 경계다 (미해결)

### 증상

ESLint `tests/lib/shared/string-utils.js`의 `"👍"` property symbol range가 `[72,2,6]`으로 나왔다. UTF-16
code unit으로는 맞지만 UTF-8 byte 기준 end는 8이다. raw 전수 검사에서 같은 invalid UTF-8 boundary가
8건 발견됐다.

### 영향

이모지처럼 supplementary-plane 문자가 occurrence 앞이나 안에 있으면 definition/use evidence가 빠질 수 있다.
현재 adapter는 invalid span을 거부하므로 잘못된 confirmed 사실은 만들지 않지만 completeness가 낮아진다.

### 근본 원인

scip-typescript 0.4.0/TypeScript compiler 좌표는 UTF-16 code unit 기반인데 공통 SCIP adapter는 UTF-8 byte
column으로 해석한다. C#에서 확인한 것과 같은 provider-boundary 단위 불일치다.

### 현재 검증 결과

- raw: documents 1,456 / occurrences 373,925 / relations 25,648
- 빈 fixture 파일 18개의 line 오류는 실제 코드 정의가 아닌 `0:0` document sentinel이다.
- 별도 invalid UTF-8 boundary 8건은 non-BMP property symbol이다.
- 최종 canonical source evidence 5,166개는 digest/line/column/byte-offset 오류와 zero-length가 모두 0이다.

### 후속 해결 gate

1. scip-typescript provider/version에 한정해 occurrence와 enclosing range를 UTF-16→UTF-8로 변환한다.
2. BMP 한글, surrogate-pair 이모지, 이모지 앞 ASCII/탭, invalid column fixture를 추가한다.
3. language cache contract marker를 올린다.
4. TypeScript/NestJS와 JavaScript/ESLint의 cold/warm을 모두 다시 실행한다.
5. raw와 canonical 전수 source-byte 검증 및 digest 결정성을 다시 통과해야 한다.

---

## TS-2026-08-09-79 — Windows 8.3 임시 경로 표기를 provider 경로의 정답으로 비교하지 않는다

### 증상

로컬에서는 코드 엔진 350개 테스트가 통과했지만 GitHub Windows runner에서는 provider root 관련 테스트
3개가 실패했다. 실제 경로는 같았으나 기대값은 `C:\Users\RUNNER~1\...`, resolver 결과는
`C:\Users\runneradmin\...`였다.

### 영향

provider 탐색과 release build는 정상이지만 깨끗한 Windows CI가 실패한다. 더 위험한 수정은 제품의
`canonicalize`를 제거해 같은 파일을 서로 다른 경로 identity로 취급하게 만드는 것이다.

### 잘못 짚기 쉬운 원인

provider 우선순위나 manifest resolution 오류가 아니다. Windows가 같은 디렉터리를 8.3 short name과 long
name 두 형태로 표현했고, 테스트가 입력 문자열을 resolver의 canonical 결과와 직접 비교한 것이 원인이다.

### 근본 원인과 수정

`std::env::temp_dir()`가 반환한 lexical path가 모든 Windows 환경에서 canonical path와 같다는 잘못된
가정이 테스트에 있었다. 제품 resolver의 보안·identity 경계는 유지하고, 기대 경로도
`managed_provider_root()`를 통과시킨 뒤 상대 launcher 경로를 붙이도록 수정했다.

### 검증 결과와 재발 방지

- 기본 병렬 실행으로 코드 엔진 350/350 통과
- `clippy --all-targets -- -D warnings` 통과
- provider 경로 테스트는 문자열 표기가 아니라 resolver와 같은 canonical boundary를 기준으로 비교한다.
- path display가 아니라 파일 identity가 목적일 때 short/long path, `\\?\` prefix, drive-letter case를 별도
  semantic 값으로 취급하지 않는다.

---

## TS-2026-08-09-80 — canonical을 쓰면서 legacy JSON도 만드는 이중 파이프라인을 유지하지 않는다

### 증상

앱은 canonical SQLite만 사용하지만 code engine과 release gate가 동시에 `language-index`,
`architecture-index`, `collection-report`를 만들거나 읽었다. 같은 provider 결과가 두 그래프 표현으로
분기됐고, 삭제 예정 출력의 PowerShell gate가 새 CLI의 `--out` 거부 뒤 전부 깨졌다.

### 영향

정답 주인이 둘로 보이고, 한쪽만 고치면 테스트와 실제 앱이 서로 다른 결과를 검증한다. 구형 구조 약
4,600줄과 collector 약 2,700줄이 실제 제품 가치 없이 유지 비용과 회귀 면적을 늘렸다.

### 근본 원인

canonical parity를 확인하기 위한 임시 호환층을 완료 조건 없이 남겨 두고, 배포 gate도 그 임시 출력에
결합했다.

### 적용한 수정

- Test/Framework IR을 실제 Language IR/canonical linker 입력으로 연결했다.
- CLI와 desktop staging의 legacy JSON 출력·소비를 삭제했다.
- 독립 제품 consumer가 없던 `collect` command와 collector 모듈을 삭제했다.
- 15개 legacy JSON gate를 제거하고, signed provider gate를 10언어 canonical publication과 독립 bundle-byte
  determinism으로 교체했다.
- 현재 계약 문서에서 canonical SQLite 하나만 제품 출력으로 정의했다.

### 검증 결과

- Code Memory 317/317
- canonical provider fixture project 9/9, 언어 계약 10/10
- 구형 `--out` 호출은 shadow 비교 도구와 provider 자체 옵션을 제외한 제품/release 경로에서 0

### 재발 시 점검 순서

1. 새 output을 추가하기 전에 실제 제품 consumer와 삭제 조건을 문서화한다.
2. release gate가 desktop과 같은 canonical receipt를 읽는지 확인한다.
3. `language-index`/`architecture-index` 호환 writer를 되살려 문제를 우회하지 않는다.

---

## TS-2026-08-09-81 — SQLite bundle을 선택할 때마다 전체 Vec로 읽지 않는다

### 증상

노드 하나를 선택해도 nodes, edges, evidence, coverage, capability receipts, gaps 전체를 메모리에
물질화하고 최대 두 snapshot을 캐시했다.

### 영향

선택 비용이 사용자가 연 상세 정보가 아니라 저장소 전체 크기에 비례한다. 대형 monorepo에서는 첫 선택
지연과 peak memory가 동시에 증가한다.

### 근본 원인

immutable bundle 검증과 제품 read model을 하나의 `load_snapshot()` 동작으로 결합했다.

### 적용한 수정

- immutable bundle digest 검증 cache와 graph query를 분리했다.
- map overview, selection node/evidence, adjacency/TracePath 입력을 고정된 parameterized SQLite query로 읽는다.
- 모든 query에 deterministic ordering과 명시적 limit를 둔다.
- 전체 graph/snapshot cache는 제거했다.

### 검증 결과

Tauri library 70 passed / 4 environment-only ignored. 반복 조회는 같은 결과를 반환하지만 전체 graph를
보유하지 않는 회귀 테스트가 통과한다. 실제 대형 repository latency/peak-memory 수치는 별도 scale gate에서
확정해야 한다.

---

## TS-2026-08-09-82 — 진행률만 있고 전체 분석 취소가 없으면 병렬 AI가 고아 프로세스로 남는다

### 증상

정적 sidecar와 16개 의미 분할 작업이 순차/병렬로 이어지는데 workspace 단위 취소 API가 없었다. 일부
단계만 취소하면 이미 시작된 Codex/Claude 자식이 계속 실행될 수 있었다.

### 영향

느린 분석을 사용자가 멈출 수 없고, 앱을 닫거나 재시도할 때 CPU/메모리와 CLI 작업이 남는다.

### 근본 원인

각 subprocess가 서로 다른 생명주기를 가져 “한 번 누른 분석”이라는 상위 operation identity가 없었다.

### 적용한 수정

- workspace당 하나의 operation ID와 guard를 분석 시작부터 종료까지 유지한다.
- 정적 엔진과 모든 local partition/retry/global reconciliation 자식에게 같은 operation ID를 전달한다.
- 하나의 취소 command가 해당 operation의 process tree를 종료한다.
- 취소는 오류 toast로 과장하지 않고 이전 published snapshot을 보존한다.

### 검증 결과

backend shared-operation cancellation 회귀, frontend cancel 동작, process-tree cancellation 테스트가 모두
통과한다.

---

## TS-2026-08-09-83 — 기본 분석 영수증에 상세 감사 샘플을 전부 싣지 않는다

### 증상

Language IR 기본 진행 marker가 언어별 정의·import·type 관계 요약과 source sample을 항상 직렬화했다.
저장소가 커질수록 제품이 소비하지 않는 로그가 커지고, 실패 전달 경로의 문자열 상한과 충돌할 수 있었다.

### 근본 원인

제품 완료 여부를 판단하는 bounded receipt와 개발자가 원인 분석에 쓰는 diagnostic report를 같은 구조체로
취급했다.

### 적용한 수정

- 기본 `language-ir-migration-receipt.v7`에는 identity, digest, 완료·누락·차단 수치만 남겼다.
- 언어별 요약과 source sample은 별도 `language-ir-diagnostic-receipt.v1`로 분리했다.
- 상세 자료는 `CODE_MEMORY_LANGUAGE_IR_DIAGNOSTICS=1`일 때만 별도 marker로 출력한다.
- 두 receipt의 분리는 Language IR stream, semantic digest, canonical SQLite bytes에 영향을 주지 않는다.

### 검증 결과

기본 JSON에 상세 sample field가 없고 opt-in diagnostic JSON에는 보존되는 계약 테스트를 추가했다. 동일 입력
bundle-byte 결정성 gate로 semantic 결과가 바뀌지 않음을 함께 확인한다.

---

## TS-2026-08-09-84 — Windows 긴 경로 임시 fixture는 검증된 Temp 범위에서 네이티브 삭제한다

### 증상

결정성 검사는 실제로 통과했지만 마지막 `Remove-Item -Recurse`가 긴 canonical cache 경로에서 실패해 테스트가
오류로 끝나고 임시 폴더가 남았다.

### 근본 원인

제품 bundle은 Windows extended-length path를 올바르게 사용했지만 PowerShell cleanup만 일반 경로 재귀 삭제에
의존했다.

### 적용한 수정

- 삭제 대상의 절대 경로가 OS Temp 아래이며 전용 fixture prefix를 갖는지 먼저 검증한다.
- Windows에서는 `\\?\` 경로와 `.NET Directory.Delete(path, recursive: true)`를 사용한다.
- 결정성·10언어 gate 양쪽에서 같은 fail-closed cleanup 규칙을 사용한다.

### 검증 결과

독립 cache 2회에서 semantic digest와 SQLite bytes가 같았고 fixture cleanup도 정상 완료했다. 기존에 남은
fixture 하나도 같은 범위 검증 후 삭제했다.

---

## TS-2026-08-09-85 — 고정한 provider의 실제 CLI 계약만 전달한다

### 증상

전체 Rust·frontend·database 검증은 통과했지만, 깨끗한 CI 환경의 canonical 결정성 smoke에서
`scip-typescript 0.4.0`이 `unknown option '--workspace-root'`로 종료됐다. strict gate는 provider 실패를
정상적으로 감지해 snapshot 게시를 차단했다.

### 근본 원인

runner가 TypeScript 작업 경계를 설정하며 공식 0.4.0 CLI의 `--cwd`와 존재하지 않는
`--workspace-root`를 함께 전달했다. 로컬의 이미 준비된 sidecar 또는 다른 wrapper만 실행하면 고정한 npm
provider와의 계약 불일치를 놓칠 수 있었다.

### 적용한 수정

- configured project와 generated source-only 경로가 공통 helper를 통해 `--cwd <analysis-root>`만 전달한다.
- 공통 helper의 argument list에 `--workspace-root`가 없음을 회귀 테스트로 고정했다.
- process cwd, provider `--cwd`, execution-context receipt의 AnalysisPlan root는 계속 같은 경계를 가리킨다.

### 검증 결과

고정한 `@sourcegraph/scip-typescript@0.4.0`과 새 release engine을 직접 조합한 두 독립 cache 실행에서
semantic digest와 canonical SQLite bytes가 동일했고 strict 결정성 gate가 통과했다.

### 재발 시 점검 순서

1. staged sidecar가 아니라 이번 source로 빌드한 engine을 `-EnginePath`로 명시한다.
2. CI가 설치하는 정확한 provider version의 `index --help`와 runner argument를 대조한다.
3. provider 실패를 fallback 성공처럼 취급하지 말고 strict gate의 원래 오류를 먼저 확인한다.

---

## TS-2026-08-09-86 — Tauri JSON merge에서 뺄 resource는 `null`로 삭제한다

### 증상

canonical engine과 결정성 smoke까지 통과한 깨끗한 CI에서 desktop Clippy만
`resource path engines\\provider-bundles doesn't exist`로 실패했다. 개발 PC에는 ignored provider bundle이 이미
있어 같은 오류가 보이지 않았다.

### 근본 원인

debug/lint/test/internal build가 큰 provider bundle을 제외하려고 `TAURI_CONFIG` override에서 해당 key를
생략했다. 그러나 Tauri는 override를 RFC 7396 JSON Merge Patch로 병합하므로, 생략한 object key는 삭제되지
않고 base `tauri.conf.json`의 값이 그대로 남는다.

### 적용한 수정

skip-provider 설정이 `bundle.resources["engines/provider-bundles"] = null`을 명시해 base key를 실제로
삭제한다. 배포 release 경로는 이 override를 사용하지 않으므로 실제 provider bundle 계약은 유지된다.

### 검증 결과

`CODEBASE_WORKSPACE_SKIP_PROVIDER_RESOURCES=1`을 둔 CI와 동일한 desktop `cargo clippy --all-targets --
-D warnings`가 통과했다. 최종 CI에서 빈 checkout의 존재하지 않는 provider directory 조건을 다시 검증한다.

---

## TS-2026-08-09-87 — 앱 루트 CSS를 Fluent portal provider에 전파하지 않는다

### 증상

왼쪽 도구 레일의 구조 지도·검색·데이터베이스·프로젝트 추가·설정 버튼에 포인터를 올리기만 해도
hover 배경과 툴팁이 반복해서 켜졌다 꺼졌다 했다.

### 근본 원인

앱 최상위 `FluentProvider`에 준 `.fluent-root` 클래스에 `width: 100%`와 `height: 100%`를 전역으로
적용했다. Fluent UI Tooltip이 portal 안에 만드는 보조 `FluentProvider`도 같은 클래스를 물려받아 전체
화면 크기의 투명한 레이어가 되었고, 툴팁이 열릴 때마다 원래 버튼의 pointer hover를 가로챘다.

### 적용한 수정

- 앱 크기와 배경 스타일은 `#root > .fluent-root` 직계 자식에만 적용한다.
- 세로 도구 레일의 툴팁은 `positioning="after"`로 버튼 오른쪽에 표시한다.

### 검증 결과

실제 브라우저 포인터로 도구 5개를 각각 8회 연속 표본 추출했다. 모든 버튼이 8/8 hover를 유지했고,
portal provider가 포인터를 가로챈 횟수는 0회였다. TypeScript typecheck, ESLint, Vitest 9개, Prettier,
Knip, production build도 모두 통과했다.

### 재발 시 점검 순서

1. `document.elementsFromPoint()`로 포인터 최상단 요소가 `fui-FluentProvider`인지 확인한다.
2. 앱 컨테이너용 전역 클래스가 Tooltip·Popover·Menu portal provider에도 적용되는지 확인한다.
3. 툴팁 위치만 바꿔 증상을 가리지 말고 portal wrapper의 실제 computed size와 pointer hit-test를 확인한다.

---

## TS-2026-08-09-88 — 검증 실패는 동일 분석 재실행이 아니라 verifier-guided repair로 교정한다

### 증상

16개 의미 partition 중 하나가 `EvidenceMismatch`로 거부됐다. AI가 area에 배정되지 않은 region까지
지나는 `representativeTracePathIds`를 그 area의 대표 근거로 선택했고, 동일 prompt 1회 재시도도 다른
area·trace 조합으로 같은 계약을 위반했다.

### 영향

정적 Fact와 성공한 partition cache는 안전했지만 전체 의미 지도는 게시되지 않았다. 같은 prompt를 다시
보내는 방식은 일시적 CLI 실패에는 유효해도, 모델이 이해하지 못한 검증 규칙을 스스로 고칠 정보가 없어
시간과 provider 호출을 한 번 더 소비했다.

### 근본 원인

- prompt는 대표 근거가 area를 직접 뒷받침해야 한다고만 썼고, trace를 소유한 모든 region이 area의
  direct·descendant member 집합 안에 있어야 한다는 verifier의 정확한 부분집합 규칙을 명시하지 않았다.
- 검증 실패와 provider 실행 실패를 같은 retry로 취급했다.
- 두 번째 호출에는 첫 rejected JSON과 `SemanticCompileError { code, path, message }`가 전달되지 않았다.

### 적용한 수정

- prompt policy를 `base-semantic-policy-v3`로 올리고 cross-area trace 금지, 빈 대표 경로 허용, 근거를
  합법화하기 위한 membership 변경 금지를 명시했다.
- 검증 실패는 원래 요청, rejected JSON, exact verifier 오류를 받는 bounded repair prompt로 전환했다.
- repair는 오류와 무관한 assignment·계층·이름·요약·근거를 보존하고 전체 교정 JSON을 반환한다.
- 실행 결과 자체가 없는 CLI 실패만 원래 prompt를 한 번 다시 실행한다.
- local partition뿐 아니라 direct 분석과 global reconciliation에도 같은 repair 경계를 적용한다.
- repair 결과는 별도 우회 없이 기존 strict schema·identity·evidence verifier 전체를 다시 통과해야 저장된다.

### 검증 결과

cross-area trace 규칙, rejected output+오류가 repair prompt에 포함되는 계약, reconciliation 원문 보존,
검증 실패와 실행 실패의 recovery 분기를 회귀 테스트로 고정했다. semantic compiler 24개와 Tauri 71개가
통과했고 각각 외부 환경 의존 2개·4개를 기본 suite에서 제외했다. 양쪽 clippy `-D warnings`와 fmt가
통과했으며, 실제 Codex CLI도 의도적으로 거부한 cross-area trace JSON을 25초 안에 교정하면서 assignment,
영역 이름·요약, 나머지 근거를 그대로 보존했다.

### 재발 시 점검 순서

1. 로그 phase가 검증 실패에 `repair`, 결과 없는 실행 실패에 original retry를 사용하는지 확인한다.
2. repair payload에 original request, verifierError, rejectedOutput 세 항목이 있는지 확인한다.
3. trace ID가 등장하는 모든 `input.regions[].representativeTracePathIds`의 region 집합과 area의 effective
   member 집합을 비교한다.
4. repair가 assignment나 hierarchy를 바꿔 근거를 억지로 합법화하지 않았는지 revision diff를 확인한다.
5. 교정 결과가 기존 verifier를 우회하거나 실패한 전체 revision을 publish하지 않았는지 확인한다.

### 남은 한계 또는 후속 gate

prompt policy version 변경으로 기존 semantic cache는 한 번 무효화된다. 실제 대형 저장소에서 새 분석을
한 번 실행해 repair 호출 수, 교정 성공률, 추가 wall time을 opt-in E2E 영수증으로 남겨야 한다.

---

## TS-2026-08-09-89 — fail-fast 검증의 다음 오류까지 재분석 없이 교정한다

### 증상

16개 의미 partition의 첫 분석 뒤 5개가 repair로 들어갔고, 한 partition은 첫 결과와 AI 교정 결과가 모두
`MissingReference`로 거부됐다. 첫 오류는 `areas[region-8ac...].parentProposalKey`, 교정 뒤 오류는 같은
누락 부모를 가리키는 다른 `areas[region-f046...].parentProposalKey`였다.

### 근본 원인

검증기는 안전하게 fail-fast 하므로 rejected JSON 안에 잘못된 부모 참조가 여러 개 있어도 첫 항목 하나만
반환했다. 기존 repair 정책은 exact error의 최소 수정을 강조해 AI가 보고된 한 경로만 고쳤고, 동일 결과에
남아 있던 두 번째 위반은 교정 결과를 다시 검증할 때 비로소 드러났다. repair 호출 자체는 정상 작동했지만
한 번만 허용해 다음 오류를 고칠 기회가 없었다.

### 적용한 수정

- `MissingReference`·`InvalidHierarchy`이면 rejected JSON을 typed proposal로 다시 읽어 존재하지 않는 parent,
  잘못된 L0/L1 parent 형태, 존재하지 않는 assignment area를 최대 128개까지 결정적으로 열거한다.
- repair payload에 primary `verifierError`와 `relatedVerifierErrors`를 함께 넣고 같은 invariant의 모든 반복
  위반을 한 번에 고치도록 명시한다.
- L0는 `level: 0 + parentProposalKey: null`, L1은 같은 `areas` 배열에 실제 존재하는 parentless L0의 정확한
  `proposalKey`만 참조한다. region ID·label·누락 area를 parent처럼 쓰지 못하게 한다.
- 첫 교정이 다른 후속 검증 오류를 드러내면 최신 rejected JSON과 최신 exact error로 2차 교정한다. 최대
  교정 횟수는 두 번이며 전체 분석 prompt는 다시 실행하지 않는다.
- provider 실행 결과가 없을 때만 실패한 exact prompt를 한 번 재시도한다. repair 실행이 죽으면 원래 분석으로
  되돌아가지 않는다.

### 검증 결과

같은 missing parent를 가리키는 두 L1 area fixture에서 repair payload가 두 경로를 모두 열거하는 회귀 테스트가
통과했다. semantic compiler 25개, Tauri 72개가 통과했고 외부 환경 의존 테스트는 각각 3개·4개 제외했다.
앱과 같은 최신 Codex CLI `0.147.0-alpha.6.5`와 `GPT-5.6 Terra`를 사용한 opt-in 실제 모델 테스트에서도
두 누락 부모 참조를 한 번에 교정하고 기존 assignment·label·summary를 보존한 채 strict verifier를 통과했다.

### 재발 시 점검 순서

1. repair payload의 `relatedVerifierErrors`에 같은 누락 parent를 쓰는 모든 area가 들어가는지 확인한다.
2. 로그에 첫 repair batch 뒤 실패 partition만 두 번째 `phase: repair` batch로 들어가는지 확인한다.
3. 두 번째 교정도 실패하면 오류 문자열에 첫 결과, AI 1차 교정, AI 2차 교정의 서로 다른 path가 남는지 본다.
4. 성공한 partition cache가 다시 실행되지 않고 실패한 partition만 provider로 전달되는지 확인한다.
5. repair 결과가 assignment나 의미 텍스트를 불필요하게 바꾸지 않았는지 revision diff를 확인한다.

### 남은 한계 또는 후속 gate

모든 verifier 규칙을 다중 오류 수집기로 복제하지 않는다. 현재는 기계적으로 안전하게 열거할 수 있는
계층·참조 위반만 묶고, 그 밖의 순차 오류는 최대 2차 교정으로 처리한다. 이 한도를 넘으면 비용을 계속
소비하지 않고 전체 revision 게시를 차단한다.

---

## TS-2026-08-09-90 — 단일 861KiB 전역 통합을 compact 계층 MapReduce로 바꾼다

### 증상

16개 local 의미 분석과 교정이 모두 끝난 뒤 provider 호출 전에
`InvalidPacket at reconciliationPrompt: global reconciliation prompt is 861420 bytes and exceeds the 524288 byte safety budget`
으로 종료됐다. 정적 Fact와 local cache는 저장됐지만 의미 지도를 게시하지 못했다.

### 잘못 짚기 쉬운 원인

512KiB 상수를 1MiB로 올리거나 전체 결과를 파일로 전달해도 모델이 읽는 양과 마지막 단일 Reduce 병목은
그대로다. local AI 호출 수나 정적 분석 속도 문제가 아니라 `Map 16개 → Reduce 1개`의 shuffle payload가
중복 데이터를 다시 펼친 문제였다.

### 근본 원인

전역 payload가 local approved revision의 `areaId`, `parentAreaId`, direct/effective member 양쪽,
별도 assignment, 전체 project citation을 함께 보냈다. boundary relation도 의미 결속도에 필요 없는 bundle
ID, 대표 edge ID, evidence ID까지 다시 실었다. 결과적으로 local 결과 16개의 실제 의미보다 내부 저장 표현과
긴 SHA-256 ID 중복이 더 큰 비중을 차지했고 Reduce만 단일 호출로 남았다.

### 적용한 수정

- shuffle 계약을 별도 compact read model로 만들었다. local area는 partition-local 정수 index,
  parent index, label/summary/category, direct member region, exact 대표 fact/trace/evidence만 전달한다.
- effective member와 assignment는 direct member+부모 관계로 재구성 가능하므로 전송하지 않는다. local
  approved area ID와 partition key도 모델 입력에서 제거한다.
- boundary relation은 source/target region과 family·truth·count만 남기고 bundle/대표 edge/evidence 목록을
  제거한다. source excerpt는 Reduce 어느 단계에도 재전송하지 않는다.
- 입력이 4개보다 많으면 fan-in 4로 결정적 묶음을 만들고 중간 Reduce를 최대 4개 병렬 실행한다. 각 중간
  결과는 해당 region scope의 원래 Fact packet verifier를 전부 통과해야 다음 단계 입력이 된다.
- 어느 중간 prompt든 512KiB를 넘으면 provider 호출 전 결정적으로 반으로 나눈다. 두 입력조차 한도에
  못 들어오면 조용히 누락하지 않고 실패한다.
- 중간·최종 Reduce도 동일한 verifier-guided repair와 최대 2차 교정 계약을 사용한다.

### 검증 결과

실제 실패 workspace의 16개 v3 partition cache를 그대로 읽어 측정했다. 기존 전역 prompt `861,420B`는
compact 계약에서 `335,304B`로 61.1% 감소했다. 계층 Reduce 1단계는 16→4로 계획됐고 prompt 크기는
각각 `81,506B`, `81,737B`, `70,330B`, `53,093B`로 모두 512KiB보다 충분히 작았다. 작은 독립 fixture의
compact reconciliation을 최신 Codex CLI `0.147.0-alpha.6.5`, `GPT-5.6 Terra`, high로 실제 실행해 전체
region assignment와 strict verifier 통과를 확인했다.

### 재발 시 점검 순서

1. 로그의 `phase: reduce`에서 각 중간 prompt `inputBytes`와 worker 수를 확인한다.
2. compact payload에 `areaId`, `effectiveMemberRegionIds`, `assignments`, `representativeEdgeIds`, source excerpt가
   다시 들어오지 않았는지 회귀 테스트를 확인한다.
3. 각 Reduce level의 입력 합집합이 전체 region과 같고 교집합이 비어 있는지 확인한다.
4. 중간 결과가 scoped verifier를 우회하거나 최종 결과가 원래 full packet digest가 아닌지 확인한다.
5. 단순 byte 상향이나 임의 truncation으로 오류를 숨기지 않았는지 확인한다.

### 남은 한계 또는 후속 gate

local Map cache는 그대로 재사용하지만 중간 Reduce 결과의 immutable cache는 아직 없다. 최종 통합 실패 뒤
재실행 비용을 줄이려면 child revision ID까지 포함한 reduce cache key가 필요하다. 또한 현재 192개 static
region directory ceiling은 계층 Reduce와 별개로 유지되며 S/M/L 실저장소에서 품질·시간·비용을 측정한 뒤
조정한다.

---

## TS-2026-08-09-91 — 대형 unit의 source inventory를 결정적·메모리 제한 병렬 단계로 바꾼다

### 증상

provider cache가 적중해도 Language IR adapter가 한 unit의 모든 파일을 순서대로 다시 열고 AST를 파싱했다.
C# EF Core 기준 source inventory만 28.533초였고 provider 실행이 0ms인 warm 전체도 168.572초였다.

### 잘못 짚기 쉬운 원인

LSP 요청 batch는 이미 bounded in-flight 방식이었다. 언어 단위만 병렬화해도 하나의 C#/Java unit에 수천
파일이 몰리면 이 직렬 구간은 그대로 남는다. 정확도 규칙을 줄이거나 AST inventory를 생략하는 것도 허용할
수 없다.

### 근본 원인

파일 load, tree-sitter parse, definition/type/import inventory와 exact import resolution은 파일별로 독립인데
unit 전체를 하나의 `for` loop에서 실행했다. 반면 결과 순서는 Language IR digest 계약에 포함되므로 완료
순서대로 곧바로 stream에 쓰는 단순 병렬화는 비결정적이었다.

### 적용한 수정

- 32파일 이상 unit만 파일별 worker pool을 사용한다.
- worker 상한은 논리 CPU에서 하나를 남긴 값, 최대 8, 가용 메모리, 가장 큰 source의 24배 AST 추정치 중
  가장 작은 값이다. `CODE_MEMORY_MAX_LANGUAGE_IR_WORKERS`는 확대가 아니라 추가 안전 상한으로만 작동한다.
- 각 worker는 완전한 파일-local inventory와 typed failure receipt를 반환한다. coordinator는 Analysis Plan의
  repository path 순서로 결과를 재조립한 뒤 한 번만 canonical sort/dedup한다.
- Language IR JSON record buffer를 재사용하고 artifact writer buffer를 1MiB로 올려 레코드별 allocator/syscall
  비용을 줄였다. digest 계산식과 fsync/publish 계약은 바꾸지 않았다.
- source inventory 구현을 `adapter/source_inventory.rs`로 분리해 거대 adapter에 병렬 제어를 섞지 않았다.

### 검증 결과

64개 TypeScript 파일을 같은 snapshot에서 worker 1과 worker 8로 각각 생성했다. stream-set digest,
semantic-payload digest, JSONL content digest, record count가 모두 동일했다. 이 작은 fixture에서 inventory
wall time은 8ms에서 3ms였고 전체 Code Memory 321개 테스트가 통과했다.

### 재발 시 점검 순서

1. `CODE_MEMORY_LANGUAGE_IR_TIMING=1`에서 `workers`, `wall_ms`, 각 `*_cpu_ms`를 분리해 본다.
2. worker 완료 순서가 아니라 repository path 순서로 merge되는지 확인한다.
3. worker panic·누락·중복 index가 전체 unit 실패로 승격되는지 확인한다.
4. worker 1/다중 worker의 네 digest/count가 같은지 회귀 테스트를 실행한다.
5. 메모리 상한을 없애거나 파일 전체 AST를 worker 밖에 장기 보관하지 않았는지 확인한다.

### 남은 한계 또는 후속 gate

8ms→3ms는 구조 검증용 작은 fixture 수치이지 EF Core 개선치를 뜻하지 않는다. C#/Java frozen corpus에서
source inventory, relation classification, stream emission, canonical linker를 다시 분리 계측해 실제 효과와
peak memory를 기록해야 이 성능 항목을 완료로 올린다.

---

## TS-2026-08-09-92 — canonical linker의 중복 전체 scan과 evidence JSON 재조회 제거

### 증상

provider와 Language IR 생성이 끝난 뒤에도 canonical publication이 Language IR JSONL을 반복해서 읽고,
definition/relation마다 SQLite evidence payload를 다시 읽어 JSON으로 역직렬화했다. 대형 저장소에서는 근거
수가 노드·관계 수와 함께 증가하므로 provider cache가 적중해도 이 비용이 그대로 남는다.

### 영향

정적 사실은 이미 만들어졌는데 최종 SQLite bundle이 늦게 공개되어 첫 분석과 warm 분석 모두 불필요하게
길어진다. 존재 여부나 source path만 필요한 조회가 full evidence payload 비용을 지불한다.

### 잘못 짚기 쉬운 원인

두 pass linker 자체가 문제인 것은 아니다. 모든 정의 identity가 등록된 뒤 관계를 해석하는 순서는 정확성을
위해 유지해야 한다. 또한 raw IR digest/record 검증 scan을 제거하면 publication 경계의 무결성이 약해진다.

### 근본 원인

raw 검증 뒤 parsed IR을 receipt/structure, definition registration, relation linking으로 세 번 읽었다. 또한
`insert_evidence`는 대부분 최초 등장인 evidence에도 먼저 `SELECT payload_json`을 실행했고, evidence 존재와
source path 확인도 매번 전체 JSON을 역직렬화했다.

### 적용한 수정

- raw IR 검증은 유지하고 parsed pass는 두 번으로 줄였다.
- 첫 parsed pass가 receipt/structure/evidence 수집과 definition identity 등록을 함께 수행한다.
- evidence가 definition 뒤에 오는 유효 stream은 pending definition으로 보류했다가 첫 pass 끝에서 등록해 IR
  record-order 계약을 좁히지 않았다.
- unique evidence는 `INSERT ... ON CONFLICT(id) DO NOTHING` fast path를 사용한다. 실제 중복일 때만 기존 payload를
  읽어 identity collision과 summary merge를 검증한다.
- SQLite staging에 `source_evidence_identity(id, path)`를 두어 존재·source path 조회에서 full JSON을 읽지 않는다.
  이 테이블은 최종 schema 정리와 `VACUUM` 전에 삭제되므로 제품 bundle 계약에는 추가되지 않는다.
- prepared statement를 재사용하고 framework/test/relation 검증도 경량 evidence identity 조회를 사용한다.

### 검증 결과

- definition이 evidence보다 먼저 오는 회귀 fixture가 정상 publication되는 것을 고정했다.
- 10언어 단일 canonical bundle 결정성 테스트와 worker 1/8의 IR·semantic·SQLite bundle digest 동일성 테스트가
  통과했다.
- Code Memory 전체 322개 테스트가 통과했다.
- 64파일 구조 fixture의 두 canonical 실행은 130ms와 140ms였다. 변경 직전 같은 fixture의 161~166ms보다
  짧지만, 이 값은 구조 회귀 관찰치이며 대형 저장소 성능 주장으로 사용하지 않는다.

### 재발 시 점검 순서

1. `CODE_MEMORY_CANONICAL_TIMING=1`로 raw verify, first parsed pass, definition materialization, relation pass,
   digest, SQLite finalize를 분리한다.
2. parsed IR 전체 scan이 두 번보다 늘지 않았는지 확인한다.
3. unique evidence 경로에서 `SELECT payload_json`이나 `FactEvidence` 역직렬화가 실행되지 않는지 확인한다.
4. duplicate evidence가 collision 검증과 summary merge를 우회하지 않는지 확인한다.
5. staging identity table이 immutable bundle에 남지 않는지 schema gate로 확인한다.

### 남은 한계 또는 후속 gate

node/edge/gap/issue upsert에는 아직 최초 레코드에서도 merge용 SELECT가 있다. 먼저 C#/Java frozen corpus에서
각 table의 unique/duplicate 비율과 canonical phase 시간을 측정한 뒤, 중복이 드문 table만 같은 fast path로
옮긴다. 의미 digest와 최종 SQLite bytes가 동일하지 않으면 채택하지 않는다.

---

## TS-2026-08-09-93 — 언어 조합별 provider 전체 복제 대신 catalog 단위 append-only pack store

### 증상

Source Census 뒤 필요한 pack만 고르는 것은 구현됐지만 app-data 대상 폴더 identity에 선택된 pack ID 전체가
들어갔다. 따라서 TypeScript 저장소, Java 저장소, TypeScript+Java 저장소를 차례로 열면 `core`, `node`,
`java`의 서로 다른 조합 루트가 생기고 이미 압축 해제한 대형 runtime bytes가 다시 저장됐다.

### 영향

정확도에는 영향이 없지만 여러 프로젝트를 쓰는 정상 사용에서 provider 준비 시간과 app-data 디스크 사용량이
언어 조합 수에 따라 증가한다. dotnet/rust/java pack은 수백 MiB이므로 조합별 복제는 최종 제품 구조가 아니다.

### 근본 원인

기존 v2 activation target이 `catalog digest + selection digest`였고, 선택한 모든 ZIP을 하나의 staging root에
합친 뒤 전체 root를 immutable publication했다. 개별 pack의 서명·경로·entrypoint 계약은 이미 독립적이지만
저장 identity가 그 독립성을 사용하지 않았다.

### 적용한 수정

- v3 store identity를 `catalog version + catalog digest` 하나로 고정했다.
- `core`는 catalog root 전체를 staging에서 원자적으로 게시한다.
- 각 language pack은 자기 ID와 같은 단일 top-level 디렉터리만 가질 수 있으며, 별도 staging에서 archive
  digest·크기·경로 이탈·symlink·unpacked byte·entrypoint를 검증한 뒤 그 디렉터리만 원자적으로 rename한다.
- catalog receipt와 pack receipt를 분리했다. 게시된 pack은 merge/overwrite하지 않고 receipt와 entrypoint
  hash를 확인한 뒤 재사용한다.
- 현재 분석은 여전히 Source Census와 교차하는 pack만 활성화·스케줄한다. 같은 catalog root에 다른 언어
  pack이 존재한다는 사실은 분석 범위를 넓히지 않는다.
- v2 데이터는 자동 삭제하지 않는다. 새 실행은 v3만 사용하며 기존 앱 데이터를 파괴적으로 정리하지 않는다.

### 검증 결과

- synthetic core/node/java pack에서 node만 설치한 뒤 같은 root에 java를 추가하고 node를 다시 요청해도
  catalog 디렉터리가 하나뿐인 것을 검증했다.
- 실제 서명된 installer core ZIP을 v3 root에 두 번 활성화해 두 호출이 같은 경로를 재사용하고 catalog/pack
  receipt와 manifest가 존재하는 것을 검증했다.
- ZIP 경로 이탈 거부와 detected-language pack 선택 회귀를 포함해 provider asset 테스트 6개가 통과했다.
- Tauri 전체 80개 중 76개 통과, 외부 환경이 필요한 4개만 의도적으로 제외됐다.

### 재발 시 점검 순서

1. app-data `managed-providers/v3` 아래 같은 catalog digest 디렉터리가 언어 조합별로 늘지 않는지 확인한다.
2. non-core ZIP이 pack ID와 다른 top-level 또는 여러 top-level을 포함하면 activation이 실패하는지 확인한다.
3. pack receipt가 staging 안에 기록된 뒤 디렉터리와 함께 rename되는지 확인한다.
4. 이미 존재하는 pack에 archive를 다시 풀거나 파일을 overwrite하지 않는지 확인한다.
5. extra installed pack 때문에 Source Census에 없는 언어 job이 schedule되지 않는지 확인한다.

### 남은 한계 또는 후속 gate

프로세스 내 activation은 mutex로 직렬화되고 cross-process rename race도 승자 결과를 재검증하지만, 두 앱
프로세스가 같은 미설치 대형 pack을 동시에 처음 요청하면 양쪽이 임시 압축 해제를 수행할 수 있다. 최종
installer 검증에서 이 빈도가 실제 문제로 확인될 때만 app-data file lock을 추가한다. 기존 v2 roots의 자동
GC도 사용자 데이터 삭제 정책과 복구 경계를 정한 뒤 별도 작업으로 다룬다.

---

## 새 중요 항목을 추가할 때 쓰는 형식

```text
## TS-YYYY-MM-DD-NN — 제목
### 증상
### 영향
### 잘못 짚기 쉬운 원인
### 근본 원인
### 적용한 수정
### 검증 결과
### 재발 시 점검 순서
### 남은 한계 또는 후속 gate
```
