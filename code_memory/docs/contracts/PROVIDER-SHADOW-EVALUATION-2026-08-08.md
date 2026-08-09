# 10-language provider shadow evaluation — 2026-08-08

## 1. 결론

지원 중인 열 언어를 같은 기준으로 다시 비교했다. 이번 조사에서 production provider를 새 후보로
교체할 수 있는 언어는 없었다.

- TypeScript와 JavaScript는 이미 사용 중인 `scip-typescript`가 가장 적합하며, 같은 provider shadow에서
  현재 사실을 100% 보존했다.
- C#은 이미 사용 중인 `scip-dotnet`이 맞지만 `--skip-dotnet-restore`는 실제 호출 사실을 누락했다.
  production runner에서 이 최적화를 제거했다.
- Python, Go, Rust, Dart의 batch SCIP 후보는 빨랐거나 더 많은 fact를 내기도 했지만 현재 confirmed fact를
  하나 이상 잃었다. 정확도 우선 계약상 승격하지 않는다.
- Java의 `scip-java`와 C/C++의 `scip-clang`은 구조적으로 좋은 후보지만 현재 Windows 완성품에 그대로
  넣을 수 있는 배포 경로가 없다.

따라서 목표 구조는 “모든 언어를 같은 도구로 강제”가 아니라 다음과 같다.

```text
언어별 가장 정확한 semantic provider
  -> 공통 Language IR
  -> 공통 evidence / gap / determinism 계약
  -> Canonical Fact Bundle
```

## 2. 이번에 만든 공통 shadow 계약

`code-memory-language compare-scip` 명령을 추가했다.

```powershell
code-memory-language compare-scip `
  --root D:\repo `
  --language go `
  --baseline D:\reports\baseline-language-index.json `
  --candidate D:\reports\candidate-index.scip `
  --out D:\reports\go-shadow.json
```

출력 schema는 `code-memory.provider-shadow-comparison.v2`다. 비교기는 provider 고유 symbol 문자열을
직접 비교하지 않는다. 같은 정의를 서로 다른 symbol scheme으로 표현할 수 있기 때문이다. 대신 workspace
정의를 exact `(path, range)` locator로 바꾸고 다음을 집합으로 비교한다.

- 승인 source document coverage
- definition location
- occurrence location + target definition + positive metadata
- relation kind + source/target definition + evidence location
- 정규화 fact 집합의 `semanticFactDigest`

판정 원칙은 다음과 같다.

1. 기존 fact 하나라도 사라지면 regression이다.
2. candidate-only fact는 실패가 아니지만 자동으로 confirmed 승격하지 않는다.
3. occurrence의 `read/write/import/definition`은 positive evidence다. false에서 true로 정교해지는 것은
   허용하지만 true를 버리면 regression이다.
4. 파일 byte digest가 달라도 normalized semantic digest가 같으면 의미 결정성은 통과한다.
5. provider 대 provider 비교는 절대 정답을 증명하지 못한다. 그래서 report의 `productionEligible`은
   의도적으로 항상 false이며 human ground truth, canonical bundle parity, packaging/security/performance
   gate까지 통과해야만 별도 승격 결정을 내릴 수 있다.

TypeScript/JavaScript는 raw SCIP occurrence에 call bit가 없어서 production이 project-model call-site
좌표를 보강한다. shadow 비교도 baseline의 **호출 위치 좌표만** 재사용한다. endpoint는 재사용하지 않으므로
candidate에 그 위치의 resolved occurrence가 없으면 관계를 만들 수 없다.

## 3. 측정 환경과 해석 제한

- OS: Windows, 2026-08-08 로컬 개발 환경
- 기본 corpus: repository 안의 언어별 reviewed fixture
- Python 추가 corpus: `D:\meeting-overlay-assistant`, 승인 Python source 884개
- 같은 candidate는 가능한 경우 두 번 실행해 output byte digest와 semantic digest를 비교했다.
- 아래 시간은 provider 교체 가능성을 찾기 위한 진단값이다. 일부 current 값은 전체 bridge 실행이고
  candidate 값은 direct indexer 실행이므로 절대 성능 SLO나 정확한 배수 비교로 사용하지 않는다.
- raw locator 차이는 실제 누락과 provider granularity 차이를 함께 포함할 수 있다. 그래서 raw gate 실패는
  “후보 폐기”가 아니라 “production 승격 금지 + canonical/human review 필요”를 뜻한다.

## 4. 언어별 실측 결과

| 언어 | 현재 truth provider | shadow 후보 | 진단 시간 | 현재 fact 보존 | 결정 | 근거 |
|---|---|---|---:|---:|---|---|
| TypeScript | scip-typescript | 동일 provider direct shadow | full 약 4.08s / direct 0.77~0.79s | definition 22/22, occurrence 40/40, relation 15/15 | 유지 | 같은 정규화 계약에서 100%; 이미 batch provider 사용 중 |
| JavaScript | scip-typescript | 동일 provider direct shadow | full 약 2.72s / direct 0.86s | 26/26, 49/49, 15/15 | 유지 | 같은 정규화 계약에서 100%; 이미 batch provider 사용 중 |
| Python | Pyright LSP | scip-python 0.6.6 최소 Windows patch | full 약 52.2s / direct 약 13.7s | 실제 repo: definition 38, occurrence 6,072, relation 139 누락 | LSP 유지 | fixture는 regression 0이지만 실제 884-file repo에서 실패; 오래된 Pyright 기반과 dependency audit 위험도 존재 |
| Java | JDTLS | scip-java 0.13.1 | JDTLS fixture 약 25.7s / candidate 실행 불가 | 미측정 | JDTLS 유지 | 공식 launcher가 Windows에서 literal `mvn`을 실행하지만 Maven 배포는 `mvn.cmd`; packaging adapter 없이 첫 compiler run 전에 실패 |
| C# | scip-dotnet | 동일 provider, restore 생략 여부 비교 | skip 약 1.63s / restore 약 2.68s | skip: 17/17, 24/25, 8/9; restore: 17/17, 25/25, 9/9 | restore 필수 | 1초를 아끼려다 실제 `Add(...)` 호출 한 개가 사라짐; production에서 skip 제거 |
| C | clangd LSP | scip-clang | candidate 실행 불가 | 미측정 | clangd 유지 | 공식 binary release가 Linux x86_64와 macOS arm64뿐이고 compile database 및 높은 메모리 예산 필요 |
| C++ | clangd LSP | scip-clang | candidate 실행 불가 | 미측정 | clangd 유지 | C와 같은 Windows packaging blocker; 자체 Windows build를 검증하지 않은 채 포함하지 않음 |
| Go | gopls LSP | scip-go 0.2.7 | full 약 3.24s / direct 0.34~0.43s | 8/10, 9/12, 3/5 | gopls 유지 | 매우 빠르고 fact도 풍부하지만 raw definition granularity와 endpoint locator가 달라 현재 fact 보존 gate 실패; canonical shadow 후 재검토 |
| Rust | rust-analyzer LSP | `rust-analyzer scip` | full 약 7.16s / SCIP cold 약 29.7s, warm 2.66~2.79s | 10/13, 13/21, 1/9 | LSP 유지 | SCIP 후보가 implementation/type relation을 현재 계약대로 보존하지 않음; 첫 실행은 dependency fetch로 더 느림 |
| Dart | Dart Analysis Server LSP | scip_dart 1.6.2 | full 약 3.61s / direct 2.16~3.00s | 11/11, 17/18, 5/6 | LSP 유지 | constructor 한 건이 `CONSTRUCTS`에서 narrower `REFERENCES`로 내려감; 후보 analyzer 세대와 bundled SDK packaging도 불일치 |

## 5. 언어별 깊은 판정

### TypeScript / JavaScript

새 provider를 찾을 문제가 아니라 current batch provider의 normalization을 정확히 재현하는 문제가 남아
있었다. call-site 좌표 보강을 shadow에도 동일하게 적용한 뒤 두 언어 모두 100%가 됐다. call endpoint를
baseline에서 복사하지 않기 때문에 가짜 합격은 아니다. 현재 선택을 유지한다.

### Python

`scip-python`은 Pyright 기반 batch indexer라 반복 LSP 질의를 없앨 잠재력은 크다. 그러나 공개 0.6.6은
Windows path separator를 정규식으로 직접 써서 실패했고, 해당 한 줄만 고친 isolated experiment에서도
실제 884-file repo의 current local truth를 보존하지 못했다.

candidate는 fact 총량이 더 많았다. 이것은 좋은 신호일 수 있지만 “더 많음 = 더 정확함”은 아니다.
외부/모호 symbol도 크게 늘었고 기존 confirmed fact가 사라졌으므로 현재는 승격하지 않는다. Pyright 최신
버전으로 단순 port하려 한 spike도 내부 API drift로 TypeScript compile error 343개가 발생해 작은 유지보수
patch 범위를 넘었다.

### Java

`scip-java`가 javac compiler plugin과 per-source shard를 사용하는 설계 자체는 정확도와 증분 처리에 매우
적합하다. 문제는 현재 official launcher의 Windows 실행 경계다. 검증한 Maven 3.9.16 ZIP은 `mvn.cmd`를
제공하지만 launcher는 Java `ProcessBuilder`로 `mvn`을 직접 찾으므로 `CreateProcess error=2`로 실패했다.
임의 shim을 production에 넣기 전에 upstream/native Windows launcher 또는 서명된 product adapter가 필요하다.

### C#

같은 provider도 실행 옵션이 사실을 바꾼다. `--skip-dotnet-restore`가 solution digest가 같을 때 안전할
것이라는 기존 가정은 틀렸다. fixture에서 실제 local call occurrence와 relation이 하나씩 사라졌다.
따라서 restore는 성능 선택지가 아니라 semantic correctness boundary로 계약했다.

### C / C++

`scip-clang`은 compile command를 그대로 사용하므로 언어 정확도 관점의 유력 후보지만 공식 Windows
binary가 없다. upstream 문서도 약 2GB RAM/core와 translation unit별 임시 공간을 요구한다. 최종 제품의
Windows provider pack에서 공급·서명·업데이트·메모리 제한을 검증할 수 없으므로 clangd를 유지한다.

### Go

`scip-go`는 가장 유망한 후보다. 두 실행의 SCIP byte hash는 달랐지만 normalized semantic digest는 같았다.
즉 protobuf serialization 차이를 비결정성으로 오판하면 안 된다. 다만 현재 raw comparator에서는 receiver
method range/field definition/endpoint convention 차이가 섞여 regression gate를 통과하지 못했다. 다음
검토는 raw provider output이 아니라 두 결과를 각각 Canonical Language IR에 넣은 뒤 해야 한다.

### Rust

bundled rust-analyzer가 직접 SCIP를 만들 수 있어 packaging 비용은 낮다. warm 실행도 빠르다. 하지만 현재
fixture에서 LSP가 제공하던 `IMPLEMENTATION`과 `USES_TYPE` 의미가 SCIP output의 `REFERENCES` 중심 표현으로
바뀌었다. 최종 시각화에 필요한 구현/타입 관계를 잃으므로 바꾸지 않는다.

### Dart

`scip_dart`는 실행되고 결정적이었으나 constructor semantics 한 건을 보존하지 못했다. 또한 제품에 묶은
Dart runtime은 Analysis Server 실행에 필요한 snapshot만 있는 최소 pack이라 `dart pub` 명령 자체가 없다.
후보를 채택하려면 full Dart SDK와 package cache까지 배포해야 하므로 작은 provider 교체가 아니다.

## 6. 코드에 반영한 것

1. 공통 `compare-scip` evidence-level shadow comparator를 추가했다.
2. raw file digest와 별개인 normalized `semanticFactDigest`를 추가했다.
3. candidate-only fact와 current-fact regression을 분리했다.
4. positive occurrence metadata가 정교해지는 경우만 허용하는 monotonic comparison을 추가했다.
5. TypeScript/JavaScript call-site normalization을 production과 shadow에서 맞췄다.
6. C# production runner에서 `--skip-dotnet-restore`와 그 stale-state cache를 제거했다.
7. production provider 선택은 그대로 유지했다. shadow를 통과하지 않은 빠른 후보는 사용자 fact를 바꾸지
   않는다.

## 7. 다음 속도 작업의 방향

이번 결과는 “모든 LSP를 SCIP로 바꾸면 빨라진다”는 접근을 기각한다. 정확도를 그대로 유지하면서 큰 폭으로
빨라질 수 있는 다음 작업은 provider 교체보다 아래 공통 구조다.

1. content/config/toolchain digest 기반 unit cache
2. 변경 unit만 다시 분석하는 persistent incremental service
3. canonical fact ownership과 transaction replacement
4. derived query와 AI semantic result의 fact-digest cache
5. Go처럼 유망한 후보의 **canonical-output shadow**

이 순서는 cold first run의 semantic provider 비용을 마술처럼 없애지는 않는다. 대신 같은 코드를 다시 읽는
중복, 한 파일 변경 뒤 전체 재분석, 지도 query 재계산을 제거하며 정확도 계약을 건드리지 않는다.

## 8. 공식 자료

- SCIP indexer 작성 원칙: https://sourcegraph.com/docs/code-navigation/writing-an-indexer
- SCIP protocol/tooling: https://github.com/scip-code/scip
- TypeScript/JavaScript: https://github.com/sourcegraph/scip-typescript
- Python: https://github.com/sourcegraph/scip-python
- Java: https://github.com/scip-code/scip-java
- Java compiler/shard 설계: https://github.com/scip-code/scip-java/blob/main/docs/design.md
- C#: https://github.com/sourcegraph/scip-dotnet
- C/C++: https://github.com/sourcegraph/scip-clang
- Go: https://github.com/scip-code/scip-go
- Rust: https://github.com/rust-lang/rust-analyzer
- Dart: https://github.com/Workiva/scip-dart
- Apache Maven release/checksum: https://maven.apache.org/download.cgi
