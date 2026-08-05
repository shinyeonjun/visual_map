# VisualMap / code_memory 트러블슈팅 기록

> 보존본. 이 파일은 2026-08-02까지 누적된 원문 개발 일지이며 현재 완료 판정 문서가
> 아니다. 원본 SHA-256은
> `F8526495879F8C372A32FF75D84E0BF1B6795AC1B14084A78674CB46FB545890`이다.
> 현재 장애와 검증 결과는 [엔진 트러블슈팅](code-memory-engine.md)과
> [2026-08-05 POC 보고서](../reports/poc-validation-2026-08-05.md)를 사용한다.

이 문서는 `D:\project`에서 진행한 VisualMap 프로젝트의 `code_memory` 엔진 개발·검증 과정에서 발생한 문제, 원인, 해결 방법, 현재 한계와 재현 명령을 한곳에 모은 기록이다.

목표는 단순히 코드를 검색하는 도구가 아니라, 실제 프로젝트의 코드를 정적으로 분석해서 다음 정보를 VisualMap이 읽을 수 있는 형태로 만드는 것이다.

- 어떤 파일과 모듈이 있는가
- 어떤 함수·클래스·컴포넌트가 있는가
- 실제 호출 대상이 무엇인가
- import가 어느 심볼 또는 외부 라이브러리로 연결되는가
- 타입·상속·generic 관계가 무엇인가
- framework의 route·component·event·RPC가 어느 코드에 연결되는가
- DB·파일·외부 라이브러리 경계가 어디에 있는가
- 위 정보를 트리와 그래프 형태로 어떻게 시각화할 것인가

---

## 1. 처음의 설계 방향과 결론

### 1.1 원래 사용하려던 codebase-memory와 실제 목적의 차이

기존 `codebase-memory-mcp`의 중심 목적은 다음에 가깝다.

```text
파일 탐색
AST·심볼 추출
기본 호출 후보 추출
빠른 검색·AI 컨텍스트 제공
```

반면 VisualMap에 필요한 목적은 다음이다.

```text
코드 전체 정적 분석
정확한 심볼·호출·import·타입 관계 추출
framework 구조 해석
DB와 연결 가능한 코드 경계 추출
트리와 그래프 데이터 생성
대규모 프로젝트에서도 반복 분석 가능
```

따라서 원본을 그대로 포크해서 계속 확장하는 것보다, 필요한 부품만 참고하거나 추출하고 VisualMap용 엔진을 별도로 발전시키는 방향이 맞다고 판단했다.

### 1.2 유지하기로 한 부품

기존 아이디어에서 재사용할 가치가 있는 부분은 다음이다.

- 프로젝트 파일 탐색
- 언어별 확장자 분류
- provider 실행 구조
- SCIP·LSP 결과를 공통 문서 모델로 변환하는 방식
- 언어별 결과 캐시
- 외부 provider를 앱 내부 `providers` 폴더에서 실행하는 방식

### 1.3 새로 필요한 부품

- framework pack 분석기
- VisualMap용 architecture tree 생성기
- provider 결과와 framework 결과 결합
- DB·파일·외부 라이브러리 경계 표현
- 결과 provenance와 checksum
- 대규모 프로젝트용 캐시와 후처리 최적화

---

## 2. VisualMap이 실제로 보여줘야 하는 것

클래스 다이어그램이나 패키지 다이어그램을 이론 그대로 재현하는 것이 목표가 아니다.

실무 개발자가 다음 질문에 바로 답을 얻는 것이 목표다.

```text
로그인 API는 어디인가?
입력값은 무엇인가?
어떤 검증 함수를 지나가는가?
어떤 서비스·모듈을 호출하는가?
어느 ORM 또는 DB 경계로 가는가?
성공·실패 결과는 어디로 흐르는가?
이 코드를 수정하면 어떤 API·화면·DB 흐름에 영향이 가는가?
```

예시 흐름:

```text
Login API
  -> 입력 데이터
  -> validation
  -> service
  -> repository / ORM
  -> database boundary
  -> token 발급
  -> response
```

그래프와 트리의 역할은 다르다.

| 자료구조 | VisualMap에서의 역할 |
|---|---|
| 트리 | 프로젝트 → 패키지 → 모듈 → 파일 → 심볼의 범위와 소속 |
| 그래프 | 함수 호출, import, 구현, route 연결, 외부 라이브러리, DB 경계 |

---

## 3. 공통 분석 구조

현재 code_memory의 전체 흐름은 다음과 같다.

```text
프로젝트 경로
  -> 분석 대상 파일 탐색
  -> 언어별 provider 선택
  -> SCIP 또는 native LSP 실행
  -> 공통 DocumentOutput 생성
  -> RelationOutput 생성
  -> framework pack 분석
  -> architecture tree·graph 생성
  -> compact JSON 저장
  -> VisualMap 또는 후속 엔진에서 사용
```

### 3.1 provider 결과

provider가 제공해야 하는 사실은 다음과 같다.

- 문서 경로
- 심볼
- 심볼 종류
- 심볼 문서·signature
- occurrence
- definition 여부
- import 여부
- read/write 여부
- enclosing range
- provider가 계산한 관계

### 3.2 code_memory가 후처리하는 관계

provider 결과를 이용해 다음 관계를 만든다.

- `CALLS`
- `IMPORTS`
- `REFERENCES`
- `IMPLEMENTATION`
- `TYPE_DEFINITION`
- `DEFINITION_OVERRIDE`

이때 provider가 확정한 관계와 code_memory가 소스 문맥으로 보조한 관계를 같은 신뢰도로 취급하면 안 된다.

### 3.3 정적 후보 관계

DB, 파일, 외부 라이브러리의 단순 문자열·호출 패턴은 다음처럼 표시한다.

```json
{
  "kind": "READS",
  "properties": {
    "resolution": "source-candidate",
    "source": "lexical-database-boundary"
  }
}
```

이 관계는 실제 DB 테이블이나 정확한 함수 심볼을 provider가 확정했다는 뜻이 아니다.

중요 원칙:

> 정확히 확인하지 못한 대상을 하드코딩해서 semantic 관계처럼 만들지 않는다.

---

## 4. SCIP와 LSP를 사용한 이유

### 4.1 SCIP

SCIP는 언어별 분석 결과를 공통 색인 포맷으로 전달하는 방식이다.

장점:

- 파일·심볼·occurrence·관계를 한 번에 받을 수 있다.
- TypeScript, Java, C# 등에서 정확한 cross-file 결과를 얻기 좋다.
- provider의 semantic 결과를 공통 형식으로 변환하기 쉽다.

한계:

- 언어별 SCIP provider가 필요하다.
- provider를 실행하려면 해당 언어의 프로젝트 환경 또는 runtime이 필요한 경우가 있다.
- provider가 분석하지 않은 파일은 결과에 존재하지 않는다.
- provider가 생성 파일이나 대형 파일을 제외하면 code_memory가 억지로 semantic 관계를 만들면 안 된다.

### 4.2 native LSP

LSP는 언어 서버가 제공하는 definition, references, call hierarchy, type hierarchy 등을 요청하는 방식이다.

주로 다음 언어에 사용했다.

- C
- C++
- Go
- Rust
- Ruby
- Dart

장점:

- 해당 언어 생태계의 공식·주요 언어 서버를 사용할 수 있다.
- definition, type hierarchy, call hierarchy를 필요한 위치에서 요청할 수 있다.

한계:

- 심볼마다 요청하면 매우 느려진다.
- 프로젝트 설정과 의존성이 맞지 않으면 결과가 줄어든다.
- LSP 결과도 언어 서버가 실제로 해석한 범위 안에서만 정확하다.

### 4.3 최종 선택

SCIP와 LSP 중 하나만 고정하지 않고 언어별로 더 안정적인 provider를 사용한다.

```text
SCIP가 안정적인 언어 -> SCIP
SCIP가 불안정하거나 provider가 부족한 언어 -> native LSP
```

---

## 5. 지원 언어 결정

초기에는 너무 많은 언어를 지원하려 했지만, 품질을 검증하지 못한 언어를 숫자만 늘려 지원하는 것은 의미가 없다고 판단했다.

현재 실제 semantic gate 대상은 12개다.

| 언어 | 방식 |
|---|---|
| TypeScript | SCIP |
| JavaScript | SCIP |
| Python | SCIP 또는 native provider 구성 |
| Java | SCIP |
| C# | SCIP .NET |
| C | native LSP |
| C++ | native LSP |
| Go | native LSP |
| Rust | native LSP |
| PHP | SCIP |
| Ruby | native LSP |
| Dart | native LSP |

제외한 언어:

- Kotlin: LSP 환경 안정성이 부족해 현재 범위에서 제외
- Swift: Windows provider 배포와 iOS SDK 의존성 때문에 현재 제외
- HTML·CSS: 현재 VisualMap의 핵심 semantic 흐름 대상이 아니므로 우선순위에서 제외

---

## 6. provider와 사용자 개발 환경 문제

### 6.1 provider가 필요한 이유

언어 분석은 단순히 확장자를 읽는 것으로 충분하지 않다.

다음 정보는 각 언어의 compiler·language server·SCIP provider가 필요하다.

- import 해석
- cross-file symbol 연결
- 타입
- 상속
- generic
- overload
- 정확한 호출 대상
- 프로젝트 설정 기반 resolution

### 6.2 사용자 환경을 바꾸지 않는 방향

사용자의 개발환경을 무조건 수정하거나 PATH를 오염시키면 안 된다.

최종 설치 구조는 다음을 목표로 한다.

```text
%LOCALAPPDATA%\VisualMap\
  providers\
    node\
    python\
    java\
    dotnet\
    clang\
    go\
    rust\
    php\
    ruby\
    dart\
```

애플리케이션은 다음 순서로 provider를 찾는다.

1. VisualMap 내부 providers
2. 사용자가 명시한 providers 경로
3. 사용자 PATH
4. 없으면 명확한 missing-tool 결과

provider는 PATH를 영구 수정하지 않고 절대 경로로 실행하는 것을 원칙으로 한다.

### 6.3 현재 provider 저장 위치

개발 중에는 다음 폴더를 기준으로 사용한다.

```text
D:\project\code_memory\providers
```

배포 시에는 설치 프로그램이 해당 내용을 다음으로 복사한다.

```text
%LOCALAPPDATA%\VisualMap\providers
```

provider 전체 용량은 약 2.78GB 수준으로 정리했다.

### 6.4 provider에 포함하면 안 되는 것

- provider의 불필요한 SDK 문서
- 예제 프로젝트
- 테스트 fixture
- 개발용 build 출력
- 중복 runtime
- 사용하지 않는 플랫폼용 바이너리
- 임시 SCIP 결과

---

## 7. CMake·C 컴파일러 설치 트러블슈팅

Windows에서 C/C++ provider를 준비하기 위해 다음 도구를 설치했다.

```powershell
winget install --id Kitware.CMake --exact
winget install --id Ninja-build.Ninja --exact
winget install --id MartinStorsjo.LLVM-MinGW.UCRT --exact
```

설치 후 확인:

```powershell
clang --version
cmake --version
ninja --version
```

확인 결과:

```text
clang version 22.1.8
cmake version 4.4.0
ninja 1.13.2
```

### 7.1 설치 직후 명령이 안 보이는 문제

winget이 다음 메시지를 출력할 수 있다.

```text
Path environment variable modified; restart your shell to use the new value.
```

이 경우 설치 실패가 아니다. PowerShell을 완전히 닫고 새로 열어야 한다.

### 7.2 CMake 사용 예시

```powershell
cmake -S D:\project\code_memory -B D:\project\code_memory\build
cmake --build D:\project\code_memory\build --config Release
```

단, 현재 code_memory의 핵심 실행 파일은 Rust crate이므로 Rust 빌드 명령은 다음이다.

```powershell
cargo test --manifest-path D:\project\code_memory\rust\Cargo.toml --all-targets
cargo build --manifest-path D:\project\code_memory\rust\Cargo.toml --release
```

---

## 8. 설치·provider 관련 주요 실패와 해결

### 8.1 PHP provider 경로 오류

문제:

기존 PHP launcher가 오래된 build 경로를 하드코딩하고 있었다.

```text
D:\project\code_memory\build\provider-tools\...
```

해결:

현재 launcher 자신의 위치를 기준으로 실행하도록 수정했다.

```text
providers\php\runtime\php.exe
providers\php\scip-php\bin\scip-php
```

관련 파일:

```text
D:\project\code_memory\providers\php\scip-php.cmd
```

### 8.2 C# provider의 solution 요구

`scip-dotnet`은 단순 `.csproj`만 있는 fixture에서 실패할 수 있다.

필요 조건:

- `.sln` 또는 `.slnx`
- solution에 프로젝트 연결

해결:

- C# semantic fixture에 solution 추가
- framework provider fixture generator에도 `fixture.sln` 추가

### 8.3 provider 경로 전달 누락

언어 semantic gate가 내부 provider 경로를 bridge에 전달하지 않아 사용자 PATH에 의존하던 문제가 있었다.

해결:

```powershell
pwsh -File tests\run-language-semantic-gate.ps1 `
  -ProvidersRoot D:\project\code_memory\providers
```

### 8.4 오래된 캐시가 빈 관계를 재사용

기존 캐시가 이전 버전의 빈 semantic 결과를 재사용했다.

해결:

- language cache version을 `v8`에서 `v9`로 변경
- provider executable bytes checksum 추가
- provider manifest checksum 추가
- 파일 내용 checksum 추가

### 8.5 framework fixture가 이전 파일을 남김

framework gate가 기존 fixture 디렉터리를 비우지 않아 다음과 같은 중복이 생겼다.

```text
src\Types.php
src\src\Types.php
```

provider가 같은 파일을 중복 처리하지 않아 partial 결과가 발생했다.

해결:

- 새 fixture를 만들기 전에 기존 fixture 폴더 삭제
- test harness의 `Write-Project` 단계에서 정리

### 8.6 PHP CodeIgniter route 파일 범위

CodeIgniter fixture의 `app/Config/Routes.php`는 PHP provider의 일반 autoload 범위에 포함되지 않았다.

해결 원칙:

- 이를 가짜 semantic 관계로 만들지 않음
- provider 범위 밖 파일로 분류
- framework pack은 source file을 직접 읽어 route fact를 만들 수 있음

### 8.7 Tauri JavaScript fixture

framework catalog에는 Tauri pack이 있었지만 provider gate fixture generator에 Tauri source template이 없었다.

해결:

```javascript
import { invoke } from "@tauri-apps/api/core";

export async function loadSessions() {
  return await invoke("list_sessions");
}
```

문자열 target이 있는 framework fact는 다음처럼 표시한다.

```json
{
  "target": "list_sessions",
  "resolution": "framework_alias"
}
```

이는 임의의 호출 대상을 만든 것이 아니라 소스에 실제 문자열 target이 있기 때문에 가능한 framework-level 해석이다.

---

## 9. fallback을 사용하지 않기로 한 이유

초기에는 provider가 실패하거나 누락하면 lexical fallback으로 관계를 채우는 방법을 고려했다.

하지만 다음 문제가 있다.

- 실제 호출인지 단순 문자열인지 구분하기 어렵다.
- VisualMap이 잘못된 그래프를 확정 관계처럼 보여줄 수 있다.
- 신뢰도 문제를 사용자에게 떠넘기는 결과가 된다.

현재 원칙:

| 상황 | 처리 |
|---|---|
| provider가 확정한 호출 | `CALLS` semantic 관계 |
| provider가 확정한 import | `IMPORTS` semantic 관계 |
| source 문자열에서 DB 호출 후보 발견 | `source-candidate` |
| source 문자열에서 파일 접근 후보 발견 | `source-candidate` |
| 외부 library alias 호출 후보 | `source-candidate` |
| framework 문자열 target | `framework_alias` |
| 아무것도 확인하지 못함 | 관계 생성 안 함 |

즉, 모르는 것을 아는 것처럼 만들지 않는다.

---

## 10. 1~14번 문제와 개선 결과

### 1. discovered files와 provider scope를 구분하지 않음

문제:

파일 탐색 결과만 보고 provider가 모든 파일을 처리했다고 가정했다.

개선:

각 언어 결과에 다음 필드를 추가했다.

```json
{
  "files_found": 2500,
  "files_indexed": 2499,
  "files_excluded": 1,
  "files_missing": 0
}
```

### 2. generated·대형 파일 처리

문제:

provider가 1MB 초과 generated 파일을 자동 제외했지만 결과가 단순 누락처럼 보였다.

개선:

- generated 경로 제외
- `.github`, `.storybook`, `e2e`, `tests`, `vendor`, `node_modules` 등 분석 제외 디렉터리 정리
- provider size limit을 `files_excluded`로 구분
- semantic fallback을 만들지 않음

### 3. JavaScript invalid-output 오분류

문제:

TypeScript 설정이 JavaScript를 포함하지 않으면 JS provider 결과가 비어 있는데 이를 오류로 표시했다.

개선:

```text
invalid-output
```

대신:

```text
excluded-by-project-config
```

으로 표시한다.

최종 Vendure 결과:

```text
JavaScript files_found=49
files_indexed=0
files_excluded=49
files_missing=0
status=excluded-by-project-config
```

### 4. 단계별 실행 시간 부재

다음 timing을 결과에 기록한다.

- `file_discovery_and_cache_lookup`
- `provider_and_scip_conversion`
- `framework_analysis`
- JSON write
- architecture 생성

### 5. framework fact가 실제 심볼로 연결됐는지 검증 부족

개선:

- framework fact에 실제 provider symbol이 있으면 연결
- 없으면 `resolution=unresolved` 또는 명시적 `framework_alias`
- framework relation은 확인된 handler에 대해서만 생성

### 6. unresolved 결과가 조용히 사라짐

개선:

- framework fact property에 resolution 기록
- 언어 파일 누락은 warning
- 프로젝트 설정 제외는 info
- unresolved를 정상 semantic 관계처럼 저장하지 않음

### 7. flow가 잘렸는데 알 수 없음

architecture flow에 다음 필드를 추가했다.

```json
{
  "truncated": true,
  "omitted_node_count": 123
}
```

### 8. framework pack이 프로젝트를 반복 순회

개선:

- 언어별 source index 생성
- framework 결과 cache 추가
- 동일한 source·provider 문서·pack·metadata checksum이면 재사용

주의:

첫 분석에서 84개 pack을 모두 검토하는 시간 자체는 아직 길다.

### 9. architecture가 언어마다 다시 생성됨

개선:

- 전체 결과 기반 architecture checksum 생성
- `%LOCALAPPDATA%\VisualMap\cache\code-memory\...`에 architecture cache 저장
- 동일 결과이면 architecture 재생성 생략

### 10. 외부 library alias 검사 반복

문제:

각 source line마다 모든 import를 다시 검사했다.

개선:

```text
source path -> imports / aliases
```

인덱스를 미리 만들어 source별 alias만 검사한다.

### 11. DB·파일·library lexical 후보가 semantic처럼 저장됨

개선:

모든 lexical boundary edge에 provenance를 추가했다.

```json
{
  "resolution": "source-candidate",
  "source": "lexical-database-boundary"
}
```

이는 db_memory가 실제 DB schema를 연결하기 전까지의 경계 후보다.

### 12. 거대한 JSON 직렬화

개선:

- 기본 compact JSON
- 필요할 때만 `CODE_MEMORY_PRETTY_JSON` 환경 변수로 pretty JSON
- `BufWriter` 사용

```powershell
$env:CODE_MEMORY_PRETTY_JSON = "1"
```

### 13. checksum 부족

language cache key에 다음을 포함한다.

- cache version
- project root
- 언어
- 소스 파일 경로
- 소스 파일 내용
- provider 실행 파일 경로
- provider 실행 파일 bytes
- provider manifest
- project configuration

framework cache에는 다음을 포함한다.

- source 내용
- provider document symbol
- 주요 metadata
- framework pack JSON

### 14. file-level incremental analysis

현재 구현 상태:

- language 단위 cache: 구현
- framework 단위 cache: 구현
- architecture 단위 cache: 구현
- 변경 파일만 provider가 재분석: 미구현

따라서 파일 하나만 바뀌어도 해당 언어 provider 전체가 다시 실행될 수 있다.

진짜 file-level incremental을 하려면 다음이 필요하다.

1. 파일별 provider 결과 cache
2. 삭제·추가·변경 파일 manifest
3. cross-file dependency 영향 범위 계산
4. 변경 파일과 영향받은 심볼만 provider에 재요청
5. 관계 제거·재생성 정책

이 작업은 현재의 project-level cache와 다른 큰 기능이다.

---

## 11. 대규모 프로젝트 실제 검증

대상:

```text
D:\visual_map_reliability_lab\vendure
```

### 11.1 최종 결과

```text
documents: 2,499
relations: 204,917
frameworks: 7
diagnostics: 1
```

TypeScript:

```text
files_found: 2,500
files_indexed: 2,499
files_excluded: 1
files_missing: 0
status: indexed
```

JavaScript:

```text
files_found: 49
files_indexed: 0
files_excluded: 49
files_missing: 0
status: excluded-by-project-config
```

### 11.2 첫 분석 시간

```text
provider + SCIP conversion: 약 39초
framework analysis: 약 8분 32초
전체: 약 9분 15초
```

현재 3~4분 목표는 첫 분석에서는 달성하지 못했다.

가장 큰 병목은 provider가 아니라 framework 최초 분석이다.

### 11.3 반복 분석 시간

같은 프로젝트를 다시 실행한 결과:

```text
전체 시간: 약 2.5초
language cache: 재사용
framework cache: 재사용
architecture cache: 재사용
```

architecture 결과도 반복 실행 간 동일했다.

---

## 12. 외부 프로젝트 실제 코드 대조

대상:

```text
D:\meeting-overlay-assistant
```

검증 결과:

| 영역 | 실제 코드와 대조한 결과 |
|---|---:|
| FastAPI route | 53개 |
| React component | 28개 |
| Tauri JS invoke | 4개 |
| Tauri Rust command | 4개 |

다음 대조가 모두 통과했다.

- route 수
- component 수
- literal invoke target
- dynamic invoke 표시
- Rust command 수
- architecture 외부 library boundary
- source range 존재

검증 스크립트:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File D:\project\code_memory\tests\compare-index-to-source.ps1 `
  -ProjectRoot D:\meeting-overlay-assistant `
  -OutputRoot D:\project\code_memory\build\external-gate-final
```

---

## 13. 전체 검증 명령 모음

### Rust unit test

```powershell
cargo test --manifest-path D:\project\code_memory\rust\Cargo.toml --all-targets
```

기대 결과:

```text
41 passed
```

### Release build

```powershell
cargo build --manifest-path D:\project\code_memory\rust\Cargo.toml --release
```

### 언어 semantic gate

```powershell
pwsh -NoProfile -File `
  D:\project\code_memory\tests\run-language-semantic-gate.ps1 `
  -ProvidersRoot D:\project\code_memory\providers
```

기대 결과:

```text
semantic gate: passed=12 skipped=0 total=12
```

### framework provider gate

```powershell
pwsh -NoProfile -File `
  D:\project\code_memory\tests\run-framework-provider-gate.ps1 `
  -ProvidersRoot D:\project\code_memory\providers
```

framework 총 개수:

```text
C             4
C++           8
C#            6
Dart          4
Go            7
Java          8
JavaScript   11
PHP           7
Python        5
Ruby          6
Rust          8
TypeScript   10
----------------
총           84
```

### 외부 프로젝트 gate

```powershell
pwsh -NoProfile -File `
  D:\project\code_memory\tests\run-external-project-gate.ps1 `
  -ProjectRoot D:\meeting-overlay-assistant `
  -ProvidersRoot D:\project\code_memory\providers `
  -OutputRoot D:\project\code_memory\build\external-gate-final
```

### 대형 Vendure 분석

```powershell
D:\project\code_memory\rust\target\release\code-memory-language.exe `
  index `
  --root D:\visual_map_reliability_lab\vendure `
  --out D:\project\code_memory\build\vendure.json `
  --architecture-out D:\project\code_memory\build\vendure.architecture.json `
  --packs-root D:\project\code_memory `
  --providers-root D:\project\code_memory\providers
```

---

## 14. 결과 파일 위치

최종 Vendure 결과:

- `D:\project\code_memory\build\vendure-final-verified.json`
- `D:\project\code_memory\build\vendure-final-verified.architecture.json`

최종 외부 프로젝트 결과:

- `D:\project\code_memory\build\external-gate-final\server-routes.json`
- `D:\project\code_memory\build\external-gate-final\web.json`
- `D:\project\code_memory\build\external-gate-final\overlay-js.json`
- `D:\project\code_memory\build\external-gate-final\overlay-rust.json`

주요 기존 계약 문서:

- `D:\project\code_memory\docs\contracts\LANGUAGE-SEMANTICS.md`
- `D:\project\code_memory\docs\contracts\LANGUAGE-PROVIDERS.md`
- `D:\project\code_memory\docs\contracts\FRAMEWORK-PACKS.md`
- `D:\project\code_memory\docs\contracts\ARCHITECTURE-INDEX.md`
- `D:\project\code_memory\docs\contracts\INSTALLATION-LAYOUT.md`

---

## 15. 문제 상황별 빠른 진단표

| 증상 | 먼저 확인할 것 | 해결 방향 |
|---|---|---|
| `missing-tool` | `providers` 경로와 provider manifest | `--providers-root` 전달 |
| 설치 후 `clang`을 못 찾음 | 기존 PowerShell 세션 | 셸 재시작 |
| C# `indexer-failed` | `.sln` 또는 `.slnx` 존재 여부 | solution 추가 |
| PHP launcher 경로 오류 | `providers/php/scip-php.cmd` | launcher 상대 경로 확인 |
| JavaScript 결과가 없음 | `tsconfig.json`, `jsconfig.json` include | 설정 제외인지 확인 |
| `indexed-partial` | `files_excluded`, `files_missing` | provider log와 실제 파일 목록 비교 |
| generated 파일 누락 | 파일 크기와 generated 경로 | fallback을 만들지 말고 excluded 표시 |
| framework fact에 symbol 없음 | `properties.resolution` | `framework_alias`인지 확인 |
| DB edge가 의심스러움 | `resolution`, `source` 속성 | `source-candidate`는 확정 관계가 아님 |
| 두 번째 실행도 느림 | cache key와 `%LOCALAPPDATA%` cache | provider·pack·source checksum 확인 |
| architecture가 매번 다시 생성됨 | architecture cache key | pack root를 동일한 절대 경로로 전달 |
| framework 최초 분석이 오래 걸림 | `framework_analysis` timing | 활성 framework pack과 source scan 최적화 필요 |
| 전체 결과 JSON이 너무 큼 | pretty JSON 여부 | `CODE_MEMORY_PRETTY_JSON` 제거 |

---

## 16. 현재 최종 판단

현재 code_memory는 다음 수준이다.

### 완료된 것

- 12개 언어 기본 semantic gate
- 84개 framework pack
- SCIP·LSP 공통 결과 변환
- provider 내부 배포 구조
- 정확한 관계와 source candidate 관계 구분
- tree·graph architecture 출력
- 결과·framework·architecture cache
- checksum 기반 cache 무효화
- 외부 실제 프로젝트 source 대조
- 대형 프로젝트 결과 생성

### 아직 완성되지 않은 것

- 첫 대규모 분석을 3~4분 이하로 줄이는 framework 최초 분석 최적화
- 변경 파일만 provider가 재분석하는 진짜 file-level incremental
- 모든 외부 라이브러리 내부 semantic 해석
- 동적 호출의 완전한 정적 해석
- ORM·DB schema와 code boundary의 완전한 연결
- 모든 framework의 모든 버전·프로젝트 관례 지원

### 다음 우선순위

1. framework 최초 분석을 8분대에서 줄이기
2. 변경 파일 manifest와 영향 범위 계산 추가
3. framework pack 활성화 조건을 dependency·metadata 기반으로 좁히기
4. DB memory 결과와 code boundary 연결
5. VisualMap에서 `source-candidate`, `framework_alias`, provider semantic 관계를 서로 다르게 표현

핵심 원칙은 계속 유지한다.

> 확인하지 못한 관계를 fallback으로 만들어 신뢰성을 낮추지 않는다.

---

## 17. 대규모 분석 구조 적용 전 확인된 트러블 목록

이 절은 아직 해결 방법을 적는 절이 아니다. 현재 테스트와 코드 분석에서 확인된 문제를 먼저 고정해 둔 기록이다.

각 항목은 대규모 분석용 MapReduce 구조를 적용한 뒤 다시 검증하고, 해결된 경우 같은 항목 아래에 해결 방법과 검증 결과를 추가한다.

### 17.1 모듈·workspace 분석 단위가 제대로 분리되지 않음

현재는 프로젝트 전체를 언어별 파일 목록으로 수집한 뒤, 언어에 따라 하나의 workspace 또는 provider 작업으로 처리하는 경로가 중심이다.

하지만 실제 프로젝트의 분석 단위는 언어마다 다르다.

```text
Go         -> go.mod / go.work / package
Java       -> Maven·Gradle module
Rust       -> Cargo crate / workspace
Dart       -> pubspec package
C#         -> csproj / sln
C/C++      -> compile_commands 단위
Ruby       -> Gemfile 단위
TypeScript -> workspace / package
```

여러 모듈이 있는 경우 현재 workspace root를 하나로 정하거나 호출한 프로젝트 루트를 그대로 사용하는 경우가 있다.

그 결과 다음 문제가 발생한다.

- 모듈별 provider 환경이 분리되지 않음
- 서로 다른 dependency 범위가 섞임
- 일부 모듈이 provider workspace에서 누락될 수 있음
- 한 모듈의 오류가 전체 언어 분석을 지연시킬 수 있음

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:1949`
- `D:\project\code_memory\rust\src\main.rs:555`

현재 상태: 미해결.

### 17.2 모듈 간 심볼·호출 관계를 안정적으로 병합하지 않음

모듈별 provider 결과를 분리해서 만든다고 해도, 최종 결과에서 단순히 문서와 관계 배열을 합치는 것만으로는 전체 그래프가 완성되지 않는다.

다음 관계를 최종 단계에서 다시 연결해야 한다.

- 모듈 A의 import와 모듈 B의 실제 심볼
- 모듈 간 함수 호출
- 모듈 간 타입·상속 관계
- 외부 package와 내부 wrapper
- 중복 심볼과 alias
- 상대 경로와 절대 경로

현재는 모듈 단위 결과를 최종 전역 symbol registry로 통합하는 별도 계층이 부족하다.

이 문제가 남아 있으면 모듈별 결과는 각각 정상이어도 최종 Visual Map에서 모듈 간 연결이 끊긴다.

현재 상태: 미해결.

### 17.3 LSP 심볼별 요청 폭발

현재 native LSP 경로는 파일을 연 뒤 문서 심볼을 조회하고, 심볼마다 references·type definition·supertypes·outgoing calls를 요청한다. 이후 소스의 lexical 후보마다 definition 요청도 추가한다.

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:1693`
- `D:\project\code_memory\rust\src\main.rs:1744`
- `D:\project\code_memory\rust\src\main.rs:1806`
- `D:\project\code_memory\rust\src\main.rs:1884`

대규모 테스트에서 실제로 다음 현상이 확인됐다.

| 프로젝트 | 결과 |
|---|---|
| `go-etcd` | 약 502초 후 중단, gopls 메모리 사용량 급증 |
| `go-prometheus` | 약 370초 후 중단, gopls 메모리 사용량 급증 |
| `ruby-redmine` | 약 407초 후 중단 |
| `c-curl` | 약 1210초 후 중단 |

MapReduce로 프로젝트를 여러 모듈로 나누더라도 각 모듈 내부에서 같은 요청 폭발이 발생하면 문제가 계속된다.

현재 상태: 미해결.

### 17.4 언어별 workspace adapter 부족

현재 provider 실행과 LSP 초기화가 상당 부분 공통 코드에 의존한다.

그러나 각 언어 provider는 서로 다른 초기화 정보가 필요하다.

| 언어 | 현재 부족한 프로젝트 정보 |
|---|---|
| Go | 여러 `go.mod`, `go.work`, build tag, package 범위 |
| Java | Maven·Gradle import, classpath, project workspace |
| Dart | 여러 `pubspec.yaml`, analysis root |
| Ruby | Gemfile, Bundler bundle 경로 |
| Rust | Cargo workspace, feature, target |
| C# | 여러 solution/project와 NuGet 관계 |
| TypeScript | monorepo package와 TS·JS 설정 분리 |

현재 공통 `initialize`와 단일 workspace folder만으로 처리하는 경로가 있어, provider가 프로젝트 전체를 정확히 이해하지 못할 수 있다.

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:2179`
- `D:\project\code_memory\rust\src\main.rs:1949`

현재 상태: 미해결.

### 17.5 외부 dependency와 프로젝트 설정 연결 부족

provider 실행 파일과 runtime은 `providers` 폴더에 포함되어 있지만, 사용자의 프로젝트가 실제로 사용하는 외부 라이브러리까지 모두 포함되어 있지는 않다.

분석 결과에 영향을 주는 프로젝트 설정 예시는 다음과 같다.

```text
package.json / package-lock.json
pom.xml / build.gradle
go.mod / go.sum
Cargo.toml / Cargo.lock
Gemfile / Gemfile.lock
pubspec.yaml / pubspec.lock
*.csproj / *.sln
```

현재 확인된 문제:

- Ruby LSP가 Bundler 실행 중 `rubygems.org`에 접근함
- Java provider가 Maven·Gradle dependency를 프로젝트별로 안정적으로 준비하지 못함
- C/C++는 compile database가 없으면 include·컴파일 옵션을 알 수 없음
- TypeScript·JavaScript는 프로젝트 설정에 따라 한쪽 언어가 제외됨

즉 provider runtime이 설치되어 있다는 사실과 프로젝트 dependency를 의미 분석할 수 있다는 사실이 분리되어 있다.

현재 상태: 미해결.

### 17.6 부분 결과가 최종 성공처럼 저장될 수 있음

언어 결과에는 다음 상태가 존재한다.

```text
indexed
indexed-partial
indexer-failed
invalid-output
excluded-by-project-config
missing-tool
```

하지만 일부 언어가 실패하거나 일부 파일이 누락되어도 전체 JSON을 만들고 명령이 정상 종료될 수 있다.

실제 테스트에서 다음과 같은 결과가 있었다.

- `nushell`: JavaScript indexer failed, Rust indexer failed
- `ruby-rails`: Ruby indexer failed
- `nopCommerce`: JavaScript indexer failed, C# partial
- `vendure`: JavaScript excluded by project config
- 대형 C/C++·Go·Java·Dart·Ruby 프로젝트: 결과 생성 전 중단

이 상태에서 architecture JSON이 생성되면, 사용자는 부분 그래프를 완성된 지도처럼 볼 수 있다.

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:867`
- `D:\project\code_memory\rust\src\main.rs:1021`
- `D:\project\code_memory\rust\src\main.rs:663`

현재 상태: 미해결.

### 17.7 checkpoint·cache가 모듈 단위 증분 분석을 제공하지 않음

현재 cache는 언어·framework·architecture 단위로 존재하지만, 파일 또는 모듈 단위로 provider 분석을 이어가는 구조는 완성되지 않았다.

현재 남아 있는 문제:

- 소스 파일 하나 변경 시 언어 provider 전체가 다시 실행될 수 있음
- nested `pom.xml`, `go.mod`, `package.json` 변경이 cache key에 완전히 반영되지 않을 수 있음
- provider workspace와 프로젝트 cache의 생명주기가 일치하지 않음
- 중간 분석 결과가 모듈별 checkpoint로 안정적으로 저장되지 않음
- 프로세스가 중단된 뒤 어느 모듈부터 재개할지 알기 어려움

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:1418`
- `D:\project\code_memory\rust\src\main.rs:1226`
- `D:\project\code_memory\rust\src\main.rs:1271`

현재 상태: 미해결.

### 17.8 framework pack이 프로젝트 전체를 반복 스캔함

현재 framework 분석은 pack을 순회하면서 프로젝트 source와 metadata를 반복 검사한다.

관련 구현:

- `D:\project\code_memory\rust\src\frameworks.rs:98`
- `D:\project\code_memory\rust\src\frameworks.rs:124`

현재 구조의 문제:

- 84개 pack이 전체 프로젝트 파일을 반복 확인함
- dependency manifest가 있어도 lexical signal 검사가 먼저 수행됨
- 동일한 source 문자열을 framework analyzer가 보관하고 architecture builder가 다시 읽음
- framework 감지와 provider semantic 성공 여부가 독립적으로 처리됨
- `packages` 등 디렉터리 제외 정책이 일반 파일 수집과 다름

Vendure 대형 분석에서 provider 변환보다 framework 최초 분석 시간이 훨씬 길게 나타난 것이 이 문제의 직접적인 증거다.

현재 상태: 미해결.

### 17.9 architecture 후처리에서 소스·인덱스를 여러 번 순회함

architecture 생성은 provider 작업이 끝난 뒤 전체 source를 다시 읽고 다음 작업을 순차적으로 수행한다.

- file tree 생성
- symbol index 생성
- import 추출
- library operation 추출
- call boundary 추출
- framework boundary 추출
- database boundary 추출
- file boundary 추출

관련 구현:

- `D:\project\code_memory\rust\src\architecture.rs:812`
- `D:\project\code_memory\rust\src\architecture.rs:841`

현재는 하나의 공유 source index·AST fact index를 사용하지 않기 때문에 대형 프로젝트에서 다음 리소스가 중복 소비된다.

- 디스크 읽기
- 문자열 보관 메모리
- 파일별 반복 순회 CPU
- 큰 결과의 serialization·deserialization 비용

참고로 `build_file_tree`가 현재 두 번 호출되는 문제는 현 코드에서 확인되지 않았다. 현재 확인된 문제는 architecture 단계 자체가 provider·framework 단계 이후 source를 다시 읽고 여러 후처리 pass를 수행한다는 점이다.

현재 상태: 미해결.

### 17.10 worker 리소스 관리와 정지 감지가 부족함

현재 provider 병렬 실행 수는 조절할 수 있지만 provider별 실제 메모리·CPU 비용을 기준으로 작업을 배치하지 않는다.

대규모 테스트에서 다음 문제가 확인됐다.

- gopls가 약 1~3GB까지 메모리를 사용함
- Java·Rust·Dart·Ruby provider가 초기화 이후 오래 진행되지 않음
- 출력이 없는 provider도 장시간 프로세스로 남음
- `CODE_MEMORY_*_TIMEOUT=0` 설정은 사실상 매우 긴 sentinel 시간으로 처리됨
- 프로세스가 정지했는지, 단순히 느린 것인지 구분할 계측이 부족함

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:620`
- `D:\project\code_memory\rust\src\main.rs:2005`
- `D:\project\code_memory\rust\src\main.rs:2783`

현재 상태: 미해결.

### 17.11 1~10번 외에 함께 확인된 관련 트러블

다음 문제는 위 10개와 직접 연결되어 있으므로 함께 추적한다.

#### C/C++ compile database 부족

`scip-clang`이 provider manifest에 없을 때 clangd fallback으로 처리되며, compile_commands가 없으면 include·매크로·컴파일 플래그가 누락된다.

관련 구현:

- `D:\project\code_memory\rust\src\main.rs:1002`
- `D:\project\code_memory\CMakeLists.txt:64`

#### Rust C 엔진과 Rust bridge 실행 경로 분리

CMake는 Tree-sitter 기반 C static library를 빌드하지만, Rust bridge는 별도의 Cargo target으로 빌드된다. 현재 두 경로가 실제 semantic 결과 생성에서 어떻게 결합되는지 명확하지 않다.

관련 구현:

- `D:\project\code_memory\CMakeLists.txt:32`
- `D:\project\code_memory\CMakeLists.txt:62`
- `D:\project\code_memory\CMakeLists.txt:64`

#### 테스트 범위 부족

현재 semantic gate와 framework gate는 작은 fixture 중심이고, `meeting-overlay-assistant` 외부 gate도 전체 프로젝트가 아니라 주요 영역을 staging해서 검증한다.

따라서 현재 테스트 통과가 다음을 보장하지 않는다.

- 전체 멀티모듈 프로젝트 완전 분석
- 1,000개 이상 파일 프로젝트의 안정성
- provider 중단 후 재개
- 모듈 일부 실패 후 최종 결과 판정
- 프로젝트 dependency가 없는 오프라인 환경
- 메모리 제한 환경

현재 상태: 미해결.

### 17.12 이 절의 기록 원칙

현재 절에서는 문제와 관찰 결과만 기록한다.

각 항목의 해결 방법은 실제 패치와 검증이 끝난 뒤 다음 형식으로 추가한다.

```text
해결 방법:

변경 파일:

검증 명령:

검증 결과:

남은 한계:
```
---

## 18. 1~10번 대규모 분석 구조 작업 결과

2026-07-28에 1~10번 문제를 실제 코드에 반영했다. 해결된 부분과 아직 완전히 끝나지 않은 부분을 구분해서 기록한다.

### 18.1 모듈·workspace planner 추가

언어별 파일 목록을 프로젝트 단위로만 처리하지 않고 프로젝트 marker를 찾아 module plan을 만든다. Go, Java, Rust, Dart, Ruby, Python, C/C++의 marker를 사용하며 각 파일은 가장 깊은 module root에 배정된다.

TypeScript·JavaScript·C#·PHP처럼 SCIP provider가 프로젝트 전체 dependency graph를 직접 관리하는 언어는 provider의 project root를 유지한다. provider가 기대하는 project graph를 임의로 잘라 semantic 정확도를 떨어뜨리지 않기 위한 제한이다.

관련 함수: plan_language_modules, module_markers, collect_module_marker_roots.

상태: 기본 module planner 구현 완료. SCIP 언어의 세부 monorepo 분할은 별도 adapter가 필요하다.

### 18.2 module 결과 병합 추가

module별 provider 결과를 전체 프로젝트 결과로 합치는 단계가 추가됐다.

- 언어별 결과 그룹화
- 문서 경로 전역화
- LSP symbol ID 경로 전역화
- 중복 문서·관계 제거
- 상태·파일 수·진단 합산
- 문서·관계 정렬

관련 함수: merge_language_analyses, rebase_language_analysis, rebase_symbol_id.

상태: 기본 병합 완료. provider가 module 밖의 대상을 보고하는 경우를 위한 완전한 cross-module semantic 재해석은 아직 남아 있다.

### 18.3 LSP 요청 제어 추가

LSP connection에 요청 예산을 추가했다.

    CODE_MEMORY_LSP_MAX_REQUESTS
    기본값: 100000
    허용 범위: 100 ~ 5000000

예산을 넘으면 native LSP request budget exceeded 오류로 중단된다.

Rust처럼 document symbols가 초기 응답에서 비어 있을 수 있는 provider는 제한된 재시도를 수행하고, 필요한 경우 workspace/symbol bulk 요청으로 심볼을 보강한다.

상태: runaway LSP 요청 방지와 Rust bulk 보강 구현 완료. 모든 언어의 심볼별 references·call hierarchy 요청을 bulk API로 바꾸는 작업은 아직 남아 있다.

### 18.4 provider workspace 보강

다음 처리를 추가했다.

- module별 provider 실행 root
- Dart analysis.setAnalysisRoots
- JDTLS module/project별 고정 workspace
- Ruby Bundler cache를 project cache 아래로 이동
- CODE_MEMORY_OFFLINE=1일 때 Ruby Bundler offline/frozen 환경 전달

JDTLS workspace는 기존 매 실행 random 경로 대신 프로젝트 cache 아래의 lsp-workspaces/java 경로를 사용한다.

상태: workspace 분리와 provider cache 경로 개선 완료. Java classpath·Gradle import, Go build tag, Dart package dependency를 완전히 해석하는 전용 adapter는 아직 남아 있다.

### 18.5 프로젝트 설정·dependency cache 수집 추가

root에 있는 설정만 보지 않고 module 하위 설정 파일도 cache key에 포함하도록 변경했다. package.json, tsconfig.json, pyproject.toml, Cargo.toml, go.mod, pom.xml, build.gradle, Gemfile, pubspec.yaml, compile_commands.json, csproj, sln 등을 포함한다.

상태: dependency 설정 변경에 대한 cache 무효화는 개선 완료. dependency를 자동 다운로드하거나 모든 외부 라이브러리의 실제 타입 정보를 provider에 공급하는 작업은 별도 범위로 남아 있다.

### 18.6 strict quality gate 추가

다음 환경 변수를 사용할 수 있다.

    CODE_MEMORY_STRICT=1

strict 모드에서는 실제 누락(`files_missing > 0`), provider 실패, invalid-output, missing-tool,
excluded-by-project-config가 있으면 최종 JSON을 저장한 뒤 프로세스를 실패시킨다.
단, C/C++의 `indexed-partial`은 현재 active compile target 밖의 파일만
`excluded / not-in-active-build`로 기록되고 `files_missing = 0`인 경우에 한해 허용한다.

compile_commands.json이 없는 C/C++ module은 저정밀 clangd fallback을 실행하지 않고 실패 결과로 기록한다.

상태: strict quality gate 구현 완료. 일반 모드의 기본값은 기존 호환성을 위해 아직 non-strict이다.

### 18.7 module cache와 cache version 강화

language cache v15, framework cache v3, architecture cache v5로 올렸다. cache key에 module root, 언어, provider executable, provider manifest, module source files, 중첩 project configuration, framework pack, architecture source snapshot을 포함한다.

상태: module 단위 cache와 재사용은 구현 완료. 파일 하나만 바뀌었을 때 영향을 받는 심볼만 재분석하는 완전한 file-level incremental은 아직 남아 있다.

### 18.8 framework pack signal cache 추가

같은 source signal과 metadata signal을 여러 framework pack이 반복 계산하지 않도록 path와 signal을 키로 하는 결과 cache를 추가했다. framework와 architecture가 각각 파일을 다시 읽던 구조도 source snapshot 공유 구조로 변경했다.

상태: 반복 문자열 검사와 framework·architecture 간 파일 재읽기 개선 완료. framework rule 자체가 AST/provider semantic 기반으로 완전히 바뀐 것은 아니다.

### 18.9 architecture source snapshot 공유

provider 결과가 만들어진 뒤 source를 한 번 읽어 SourceSnapshot을 만들고 framework analyzer와 architecture builder가 공유한다. architecture는 기존처럼 소스 파일을 다시 열지 않고 snapshot을 사용한다.

상태: 디스크 재읽기 제거와 source snapshot 공유 완료. architecture 내부의 import·library·DB·file boundary 여러 pass 자체는 아직 유지된다.

### 18.10 worker resource weight 추가

Go, Rust, Java, Dart, Ruby, C, C++ provider는 기본 weight 2, 나머지는 weight 1로 두고 CODE_MEMORY_MAX_PROVIDER_WEIGHT 기본값 4를 적용한다. CPU 코어 수만 보고 provider를 동시에 많이 실행하지 않도록 변경했다.

상태: weighted scheduling 구현 완료. 운영체제 수준의 실제 process memory peak 측정과 강제 종료는 아직 남아 있다.

## 19. 작업 후 검증 결과

Rust unit test: 42 passed, 0 failed.

12개 언어 semantic gate: passed=12, skipped=0, total=12.

meeting-overlay-assistant 외부 gate:

- FastAPI routes 53 / 53
- React components 28 / 28
- Tauri JS invoke facts 4
- Tauri Rust commands 4 / 4
- source comparison passed
- external project gate passed

Rust fresh 분석: documents 2개, relations 7개. 초기 document symbols가 비어 있는 경우에도 workspace/symbol bulk 보강 후 main -> add 호출 관계가 생성됐다.

## 20. 현재 남은 문제

1. Java·Go·Dart의 실제 dependency/classpath/build 설정 adapter
2. 모든 언어에서 LSP 심볼별 요청을 bulk 요청으로 대체
3. file-level 영향 범위 기반 증분 분석
4. provider process 실제 메모리 peak 감시
5. 외부 library 실제 타입·버전·symbol 연결
6. architecture lexical boundary의 provider semantic 전환
7. C Tree-sitter static library와 Rust bridge의 실제 실행 경로 통합

현재 표현은 다음이 정확하다.

    MapReduce-inspired module execution: 구현됨
    module 결과 병합: 구현됨
    cache·checkpoint 기반 재사용: 부분 구현
    LSP runaway 방지: 구현됨
    provider별 완전한 project semantic adapter: 미완료
    모든 대규모 프로젝트 완전 분석: 아직 미보장

## 21. 최종 재검증 기록

최종 release 재빌드 후 다음 검증을 다시 실행했다.

- `cargo fmt -- --check`: 통과
- `cargo test --all-targets`: 42 passed, 0 failed
- 12개 언어 semantic gate: 12 passed, 0 skipped
- `meeting-overlay-assistant` 외부 gate: 통과
- 외부 gate source 비교: 통과

외부 Rust provider가 Tauri 의존성 빌드 중 Windows 파일 잠금 경고를 출력했지만, 분석 결과와 source comparison은 통과했다. 이 경고는 codebase-memory의 병합 실패가 아니라 provider가 프로젝트 build artifact를 갱신하는 과정에서 발생한 환경 경고로 별도 추적한다.

## 22. 외부 라이브러리 경계 처리 보강

Visual Map용 architecture 결과에서 프로젝트 내부 코드는 provider 결과를
기준으로 연결하고, 프로젝트 밖의 import만 `EXTERNAL_LIBRARY` 노드와
`USES_LIBRARY` edge로 표시한다. 외부 라이브러리 내부를 확인하지 못했는데
내부 `CALLS` 관계인 것처럼 만들지 않는다.

추가로 다음을 보강했다.

- C/C++의 `#include <...>`와 `#include "..."` 수집
- 프로젝트 안에 실제로 존재하는 local header는 외부 경계에서 제외
- 프로젝트 manifest 이름과 일치하는 local package/module은 외부 경계에서 제외
- 외부 import edge의 `resolution`을 애매한 `source-candidate`가 아닌
  `external`로 변경
- C/C++ system header는 `external:system:<header>` 경계로 표시
- architecture 화면용 외부 노드 label을 `<package> 라이브러리` 형식으로 통일
- 그래프 ID와 실제 package name은 유지해 표시명 변경과 무관하게 연결 보존

이 단계에서 외부 package 내부 전체를 자동으로 분석하거나 dependency를
인터넷에서 내려받지는 않는다. 소스가 프로젝트에 포함되어 있으면 provider가
그 소스를 분석하고, 포함되어 있지 않으면 외부 경계로 남기는 정책이다.

## 23. provider offline 실행 정책

provider가 dependency를 확인하는 과정에서 인터넷으로 빠지지 않도록
`CODE_MEMORY_ALLOW_NETWORK=1`을 명시하지 않는 한 분석 프로세스에 offline
환경을 전달한다.

- Go: `GOPROXY=off`, `GOSUMDB=off`, `GOTOOLCHAIN=local`
- Maven/Gradle: offline 옵션 전달
- Ruby: `BUNDLE_ALLOW_OFFLINE_INSTALL=1`, `BUNDLE_FROZEN=1`
- Dart: analytics 비활성화

따라서 프로젝트 dependency가 프로젝트 안에 있거나 사용자 로컬 cache에
이미 있을 때만 provider가 활용한다. 없으면 provider 결과에 억지로 내부
호출을 만들지 않고 architecture의 외부 라이브러리 경계로 남긴다.

## 24. 정적 분석에서 동적 호출 경계 처리

실행 추적을 추가하지 않고, 소스에서 명시적으로 확인 가능한 runtime
dispatch 패턴만 `DYNAMIC_BOUNDARY` 노드와 `DYNAMIC_CALL` edge로 표시한다.
예시는 Python `getattr`/`eval`, Java `Class.forName`, JavaScript `eval`,
C/C++ `dlsym`, Ruby `send` 등이다.

대상 함수 이름을 정적으로 확인할 수 없는 경우에는 내부 `CALLS` 관계를
추측해 만들지 않는다. Visual Map은 해당 모듈에서 동적 호출이 발생한다는
사실만 보여주고, 실제 대상은 실행 없이 확정하지 않는다.

## 25. 1~8 범위 최종 상태

현재 codebase-memory의 실무 지도 범위는 다음과 같다.

- 외부 라이브러리 이름·경계: 완료
- offline provider 실행: 완료
- 명시적인 정적 동적 호출 경계: 완료
- module planner·결과 병합·공유 source snapshot: 완료
- LSP timeout·request budget·weighted provider scheduling: 완료
- Java·Go·Dart workspace root와 프로젝트 설정 checksum: 구현됨
- raw index의 파일별 `coverage`(`indexed`/`excluded`/`missing`) 기록: 완료

아직 “완전한 의미 해석”으로 부를 수 없는 항목은 다음과 같다.

- Java classpath와 Gradle/Maven dependency를 provider에 완전히 재구성하는 adapter
- Go build tag와 module cache를 provider에 완전히 재구성하는 adapter
- Dart package dependency를 provider에 완전히 재구성하는 adapter
- 외부 라이브러리의 실제 타입·내부 호출 요약
- provider를 다시 실행하지 않는 진짜 file-level incremental 분석
- OS process memory peak 감시와 memory limit 종료
- 모든 framework 버전·동적 등록 방식의 완전한 semantic 해석

따라서 현재 결과는 “큰 구조 지도” 목표에는 맞지만, 모든 언어와 외부
dependency를 compiler 수준으로 완전히 해석하는 단계는 아니다.

## 26. 2026-07-29 C/C++ 완성 기준 보강 작업

이번 작업에서는 C와 C++을 하나의 단순 확장자 분석기로 처리하지 않고,
clangd의 정확한 컴파일 컨텍스트를 기준으로 별도 언어 결과를 만들도록
보강했다.

### 26.1 컴파일 컨텍스트가 없는데 분석을 진행하던 문제

문제:

- `scip-clang`가 없으면 clangd fallback을 사용한다.
- 하지만 `compile_commands.json`, `compile_flags.txt`, `.clangd`가 없으면
  include 경로·매크로·표준 버전·컴파일 옵션을 알 수 없다.
- 이 상태에서 clangd를 실행하면 이름만 비슷한 잘못된 관계가 생길 수 있다.

해결:

- C/C++ translation unit마다 compile database 항목이 있는지 검사한다.
- compile database가 없으면 `.clangd` 또는 `compile_flags.txt`를 확인한다.
- 조건을 만족하지 못하면 provider를 억지로 실행하지 않고
  `indexer-failed`와 `no compile context` 진단을 기록한다.
- 대형 프로젝트에서도 CMakeLists.txt마다 provider를 반복 실행하지 않고,
  실제 컴파일 컨텍스트가 있는 모듈만 분리한다.

### 26.2 CMake·Meson 빌드 디렉터리를 찾지 못하던 문제

문제:

기존 compile database 탐색은 프로젝트 루트와 바로 아래 한 단계 정도만
확인했다. 따라서 다음 형태를 놓칠 수 있었다.

```text
build/Debug/compile_commands.json
out/build/compile_commands.json
cmake-build-debug/compile_commands.json
```

해결:

- `build`, `out`, `cmake-build-*`, `build-*`, `out-*` 아래의 하위 빌드
  디렉터리까지 탐색한다.
- CMake·Meson은 자체 파일을 보고 컴파일 옵션을 추측하지 않는다.
- 생성된 `compile_commands.json` 또는 명시적 compile flags를 실제 컨텍스트로
  사용한다.

### 26.3 VCXPROJ를 컴파일 컨텍스트로 잘못 취급할 위험

문제:

`.vcxproj` XML만 보고 MSVC include path, 조건부 매크로, 플랫폼,
Configuration, PropertySheet를 임의로 조합하면 정확한 분석이 아니다.

해결:

- `.vcxproj`, `.vcxproj.filters`, `.props`, `.targets`를 프로젝트 설정 및
  cache checksum 대상에 포함했다.
- VCXPROJ가 있다는 이유만으로 clangd용 compile flags를 생성하지 않는다.
- VCXPROJ 프로젝트도 `compile_commands.json`, `compile_flags.txt`, 또는
  `.clangd`가 있을 때만 정확한 C/C++ semantic 분석을 실행한다.

현재 한계:

VCXPROJ 자체를 MSBuild처럼 평가해서 compile database로 변환하는 adapter는
아직 만들지 않았다. 이는 누락이 아니라, 잘못된 MSVC 옵션을 추측하지 않기
위한 의도적인 차단이다.

### 26.4 헤더와 translation unit 중복 문제

문제:

- `.h`가 C와 C++ 확장자 목록에 동시에 포함될 수 있었다.
- `.inc`, `.inl`, `.ipp`, `.tpp` 같은 헤더 조각을 독립 파일처럼 clangd에
  열면 실제 include 문맥과 다른 결과가 나올 수 있었다.
- coverage에는 같은 헤더가 C와 C++ 양쪽에 중복으로 나타날 수 있었다.

해결:

- 헤더 조각은 standalone semantic open 대상에서 제외한다.
- C++ translation unit이 있는 모듈에서는 공유 헤더를 C++ 쪽에서 소유한다.
- 최종 coverage는 파일 경로당 한 건만 남긴다.
- 최종 documents와 relations도 동일한 key 기준으로 deduplicate한다.

### 26.5 include 내부·외부 구분 문제

문제:

`#include "common.h"`가 프로젝트 내부 파일인지 외부 라이브러리인지
단순 문자열만으로 확정하기 어려웠다.

해결:

- 정확한 프로젝트 상대 경로를 먼저 확인한다.
- 현재 파일의 부모 경로를 기준으로 확인한다.
- 프로젝트 전체에서 unique한 suffix만 내부 파일로 연결한다.
- 같은 이름의 후보가 둘 이상이면 외부 경계로 남겨 잘못된 내부 연결을
  만들지 않는다.

### 26.6 C 호출 관계가 references와 섞이던 문제

문제:

clangd의 `textDocument/references` 결과를 callable symbol에도 적용하면
호출이 아닌 일반 참조가 `CALLS`로 잘못 분류될 수 있었다.

해결:

- clangd callable 관계는 `callHierarchy/outgoingCalls`를 사용한다.
- clangd references 요청은 타입 계층 심볼에만 사용한다.
- C/C++ source range는 provider가 반환한 occurrence에 대해서만 호출 여부를
  분류한다.
- 이름 매칭만으로 대상 symbol을 만들지 않는다.

### 26.7 C typedef·struct와 C++ type 관계 부족

해결:

- SCIP symbol kind 중 class, struct, interface, enum, type alias,
  type parameter를 type symbol 집합으로 수집한다.
- C/C++ occurrence가 provider가 보고한 type symbol을 가리키면
  `USES_TYPE` 관계로 저장한다.
- VisualMap architecture 변환 시 `USES_TYPE` summary edge로 변환한다.

### 26.8 C++ 상속·override 부족

해결:

- C++ class/type symbol에 대해 clangd type hierarchy의 supertype을 요청한다.
- `textDocument/implementation`으로 virtual/override 구현 대상을 요청한다.
- 결과는 `IMPLEMENTATION`으로 저장한다.
- 이름이 같다는 이유만으로 override 관계를 만들지 않는다.

### 26.9 선언·구현 연결이 일반 override로 저장되던 문제

문제:

헤더 선언에서 구현 파일의 definition으로 이동하는 관계가 기존에는
일반 `DEFINITION_OVERRIDE`와 구분되지 않았다.

해결:

- C/C++ definition query 결과를 `DEFINITION`으로 저장한다.
- architecture에서는 `DEFINES` edge로 변환한다.
- C/C++ fixture에 다음 실제 검증 사례를 추가했다.

```text
declarations.hpp: declared_value 선언
implementations.cpp: declared_value 구현
```

### 26.10 provider 오류가 파일 단위로 남지 않던 문제

문제:

clangd가 `publishDiagnostics`를 보내도 기존 bridge가 이를 버렸다. 그래서
어느 파일의 몇 번째 줄이 문제인지 raw JSON에서 알 수 없었다.

해결:

진단 JSON에 다음 필드를 추가했다.

```json
{
  "language": "cpp",
  "level": "warning",
  "message": "...",
  "path": "main.cpp",
  "line": 1
}
```

architecture diagnostic에도 같은 path와 line 정보를 전달한다.

### 26.11 provider 진단이 cache 실행에서 사라지던 문제

문제:

첫 실행에서는 clangd 진단이 있었지만, language cache를 재사용한 두 번째
실행에서는 진단이 사라졌다.

해결:

- language cache에 파일 단위 provider diagnostic을 저장한다.
- architecture cache checksum에 coverage와 diagnostic을 추가한다.
- language cache version을 `v28`로 올렸다.

검증:

```text
첫 실행 진단 수: 1
cache 재실행 진단 수: 1
```

### 26.12 대형 프로젝트에서 CMakeLists.txt마다 실패하던 문제

문제:

컴파일 컨텍스트가 없는 curl 프로젝트에서 하위 CMakeLists.txt마다 별도
모듈이 만들어져 같은 실패 진단이 반복되었다.

해결:

- 실제 `.clangd`, `compile_flags.txt`, `compile_commands.json`이 있는
  디렉터리만 독립 C/C++ 모듈로 나눈다.
- 설정 파일만 있는 하위 디렉터리는 상위 모듈에 포함한다.
- 컨텍스트가 없으면 모듈 전체를 한 번 명확하게 실패시킨다.

결과:

```text
이전: 반복되는 모듈 실패 진단
이후: C/C++ root 모듈의 명확한 no compile context 진단
```

## 27. C/C++ 이번 작업 검증 결과

### 27.1 Rust 단위 테스트

```text
52 passed, 0 failed
```

검증 명령:

```powershell
cargo test --manifest-path D:\project\code_memory\rust\Cargo.toml --quiet
cargo fmt --manifest-path D:\project\code_memory\rust\Cargo.toml --all -- --check
cargo build --manifest-path D:\project\code_memory\rust\Cargo.toml --release
```

### 27.2 C/C++ 소규모 완성 게이트

검증 fixture:

```text
D:\project\code_memory\tests\fixtures\native-lsp-c
```

검증 항목:

- C/C++ status가 `indexed`
- C 호출 관계
- typedef/struct type 관계
- C++ 상속 관계
- 선언→구현 관계
- template/overload 호출
- 중복 문서 0개
- 중복 coverage 0개
- 중복 관계 0개
- `ESTIMATED`, `GUESS`, `INFERRED` 관계 0개

결과:

```text
PASS c/cpp completion gate
```

실행 명령:

```powershell
& D:\project\code_memory\tests\gates\run-c-cpp-completion-gate.ps1 `
  -ProvidersRoot D:\project\code_memory\providers
```

### 27.3 C/C++ 대규모 안전 게이트

대상:

```text
D:\visual_map_reliability_lab\c-curl
```

검증 결과:

```text
C/C++ source files: 755
compile context: 없음
결과: 추정 분석 없이 indexer-failed 기록
실행: 약 2초
PASS large C/C++ safety gate
```

이 결과는 “755개 파일을 semantic indexed했다”는 뜻이 아니다. 컴파일
컨텍스트 없는 대형 프로젝트를 느리게 잘못 분석하지 않고 빠르게 차단했다는
뜻이다.

정확한 대형 semantic gate를 통과하려면 해당 프로젝트에서 다음 중 하나를
먼저 준비해야 한다.

```text
compile_commands.json
compile_flags.txt
.clangd
```

## 28. 이번 작업의 최종 판정

완료:

- 컴파일 컨텍스트 검출
- CMake/Meson generated compile DB 탐색
- VCXPROJ 설정 감지 및 cache 반영
- translation unit coverage 검사
- 헤더 fragment standalone 분석 방지
- 헤더 중복 제거
- 내부·외부 include 구분
- C 호출·typedef/struct 관계
- C++ 상속·override·template/overload 요약
- 선언·구현 연결
- 외부 라이브러리 경계
- provider 파일 단위 진단
- 추정 semantic 관계 차단
- 문서·관계·coverage 중복 제거
- cache 영향 범위 보강
- 소규모 완료 게이트
- 대규모 컨텍스트 부재 안전 게이트

남은 제한:

- VCXPROJ XML을 MSBuild처럼 평가해 compile database로 변환하는 기능
- compile context가 없는 대형 프로젝트의 정확한 semantic indexing
- 완전한 file-level 영향 범위 incremental 분석

따라서 현재 C/C++은 “컴파일 컨텍스트가 있는 프로젝트는 정확히 분석하고,
없는 프로젝트는 추정하지 않고 명확히 차단하는 상태”로 기록한다.

## 29. 2026-07-29 JavaScript·TypeScript 작업 기록

JavaScript와 TypeScript 쪽은 단순히 `scip-typescript`를 실행하는 것만으로
끝내지 않고, TypeScript compiler API를 이용한 프로젝트 모델을 앞단에
추가했다. 목적은 VisualMap에 필요한 큰 구조인 파일 import, 프로젝트 경계,
호출 흐름을 실제 compiler resolution 결과에 맞춰 연결하는 것이다.

### 29.1 import를 문자열로만 연결하던 문제

문제:

- `import` 문자열 이름만 비교하면 같은 파일명이나 alias를 잘못 연결할 수 있다.
- `baseUrl`, `paths`, `extends`, project references, package exports를 직접
  하드코딩하면 프로젝트마다 깨진다.
- unresolved import를 내부 파일 관계로 만들 위험이 있다.

해결:

- bundled TypeScript compiler의 `ts.resolveModuleName`을 사용한다.
- 프로젝트의 `tsconfig.json`과 `jsconfig.json` compiler options를 그대로
  사용한다.
- `extends`, `baseUrl`, `paths`, project references, package exports가
  TypeScript resolution에 맡겨진다.
- 실제 프로젝트 파일로 확인된 경우에만 `file_relations`의 내부
  `IMPORTS`를 생성한다.
- 해석하지 못한 import는 내부 관계로 만들지 않고 진단 수만 기록한다.

결과 예시:

```json
{
  "kind": "IMPORTS",
  "properties": {
    "resolution": "internal",
    "source": "typescript-module-resolution"
  }
}
```

### 29.2 TypeScript 프로젝트와 JavaScript 프로젝트가 섞이는 문제

문제:

- 하나의 대형 저장소에 여러 `tsconfig`가 있을 수 있다.
- 가장 가까운 설정 파일을 잘못 선택하면 다른 package의 compiler option을
  사용하게 된다.
- `tsconfig.json`이 JavaScript를 포함하지 않으면 JavaScript provider 결과가
  비어 있을 수 있다.

해결:

- 중첩된 `tsconfig*.json`, `jsconfig*.json`을 모두 찾는다.
- compiler API가 실제로 구성한 파일 목록을 project unit으로 만든다.
- 파일은 가장 구체적인 설정 unit에 먼저 배정한다.
- 어느 설정에도 포함되지 않은 파일은 synthetic unit으로 분리한다.
- JavaScript는 `allowJs`가 켜진 TypeScript project에서는 해당 project
  설정을 따르고, 그렇지 않으면 별도의 JavaScript 분석 unit으로 다룬다.
- synthetic config는 사용자 프로젝트가 아니라 provider work/cache 아래에
  read-only 형태로 생성한다.

사용자 프로젝트에 다음 파일을 생성하거나 수정하지 않는다.

```text
tsconfig.json
jsconfig.json
package.json
lockfile
```

### 29.3 TypeScript 호출 대상이 정확하지 않던 문제

문제:

SCIP occurrence가 함수 이름을 반환하더라도 그 occurrence가 실제 호출인지
일반 참조인지 구분해야 한다. 이름에 `()`가 있다고 단순 판단하면 property
call, constructor, tagged template, overload를 잘못 처리할 수 있다.

해결:

- TypeScript compiler `Program`의 `TypeChecker`를 사용한다.
- `CallExpression`, `NewExpression`, `TaggedTemplateExpression`을 탐색한다.
- checker가 실제 symbol 또는 resolved signature를 찾은 위치만 call range로
  저장한다.
- SCIP occurrence target이 compiler call range에 포함될 때만 `CALLS`로
  분류한다.
- unresolved call target은 `CALLS`로 만들지 않는다.

즉 구조는 다음과 같다.

```text
TypeScript AST node
  -> TypeChecker symbol/signature resolution
  -> 실제 source range 저장
  -> SCIP occurrence와 range 비교
  -> 확인된 경우에만 CALLS
```

### 29.4 TypeScript·JavaScript 파일 coverage 문제

문제:

- provider가 처리하지 않은 파일을 단순히 누락시키면 원인을 알 수 없다.
- generated, test, docs, 대형 파일, project config 제외가 서로 섞였다.

해결:

모든 발견 파일에 대해 다음 상태를 분리한다.

```text
indexed
excluded
missing
```

그리고 missing 또는 excluded 이유를 다음처럼 구분한다.

```text
provider-missing
provider-failed
project-config
provider-size-limit
generated
not-returned-by-provider
```

Vendure 대형 프로젝트 검증 결과:

```text
TypeScript files_found  = 2500
TypeScript files_indexed = 2499
TypeScript files_missing  = 0

JavaScript files_found  = 49
JavaScript files_indexed = 0
JavaScript files_excluded = 49
JavaScript files_missing  = 0
status = excluded-by-project-config
```

JavaScript가 `tsconfig`에 포함되지 않은 경우 이를 provider 오류나 가짜
semantic 관계로 바꾸지 않는다.

### 29.5 JavaScript를 TypeScript에 억지로 섞던 문제

해결:

- TypeScript와 JavaScript를 `LANGUAGES`에서 별도 언어로 유지한다.
- provider는 공통 `scip-typescript`를 사용하지만 language id와 coverage는
  분리한다.
- TypeScript가 JavaScript 파일을 project에 포함하면 compiler model이 함께
  해석한다.
- 포함되지 않은 JavaScript는 별도 allowJs 분석 unit으로 module boundary를
  계산한다.
- 결과의 language와 cache key는 `typescript`와 `javascript`를 구분한다.

### 29.6 Vue SFC 처리

문제:

`.vue` 파일은 일반 `.ts` 또는 `.js` 파일처럼 provider에 바로 넘길 수
없다. `<script>`, `<script setup>`, `<template>`이 한 파일 안에 섞여 있다.

해결:

- Vue 파일의 script block을 TypeScript virtual source로 만든다.
- script import는 TypeScript module resolution으로 해석한다.
- template에서 정적으로 확인되는 imported component tag는 내부
  `IMPORTS` file relation으로 저장한다.
- HTML 기본 tag는 component로 오인하지 않는다.
- 동적 component, global registration, Nuxt auto-import는 대상 파일을
  확정할 수 없으므로 내부 relation을 만들지 않는다.

### 29.7 외부 라이브러리 처리

문제:

`pandas`, `react`, `vue`, 사내 npm package처럼 프로젝트 밖에 있는 패키지의
내부 코드를 전부 provider에 넣으려 하면 설치 용량과 사용자 환경 의존성이
급격히 커진다.

해결:

- 프로젝트 내부 파일로 resolution되면 내부 file relation을 만든다.
- 프로젝트 내부에 없으면 package 단위 `EXTERNAL_LIBRARY` 경계로 남긴다.
- 외부 라이브러리 내부 함수·타입을 이름 매핑이나 하드코딩으로 만들지 않는다.
- 외부 package 이름은 실제 import 문자열에서 동적으로 추출한다.
- 인터넷에서 package를 자동으로 내려받지 않는다.

VisualMap에 표시할 수 있는 수준은 다음과 같다.

```text
입력 데이터
  -> 프로젝트 코드
  -> react 라이브러리
  -> 결과 컴포넌트
```

반면 react 내부 구현 함수까지 확정하려면 해당 외부 소스와 정확한 provider
project context가 별도로 필요하다.

### 29.8 대형 TypeScript 프로젝트 메모리 문제

문제:

대형 프로젝트에서 scip-typescript의 전역 source cache가 메모리를 크게
사용할 수 있다.

해결:

source file 수가 2,000개 이상이면 provider에 다음 옵션을 전달한다.

```text
--no-global-caches
```

이 방식은 일부 반복 parsing을 허용하는 대신 대형 프로젝트의 메모리 폭증을
줄이는 선택이다. 분석 결과의 정확도를 낮추는 이름 기반 fallback은 추가하지
않았다.

### 29.9 TypeScript·JavaScript cache 문제

language cache key에 다음을 포함한다.

- TypeScript/JavaScript 언어 id
- 분석 파일 상대 경로
- 파일 내용 hash
- tsconfig/jsconfig 및 nested project config checksum
- provider manifest
- provider 실행 파일 경로와 bytes
- cache version

따라서 다음 변경 시 기존 결과를 재사용하지 않는다.

- source 변경
- `tsconfig` 또는 `jsconfig` 변경
- package 설정 변경
- provider 버전 변경
- 분석 엔진 cache schema 변경

framework와 architecture 결과도 TypeScript/JavaScript document와 source
snapshot을 checksum에 포함한다.

### 29.10 TypeScript·JavaScript 검증 결과

언어 semantic gate:

```text
TypeScript: indexed
JavaScript: indexed
정확한 CALLS target/range 확인
내부 IMPORTS file relation 확인
중복 semantic document 없음
```

외부 프로젝트 대조:

```text
meeting-overlay-assistant
  React component: 28 / 28
  Tauri JavaScript invoke: 4

Vendure
  TypeScript document: 2,499
  TypeScript relation: 204,917
```

관련 gate:

```text
D:\project\code_memory\tests\gates\run-language-semantic-gate.ps1
D:\project\code_memory\tests\gates\run-external-project-gate.ps1
```

### 29.11 현재 TypeScript·JavaScript의 남은 한계

- 동적 `import()` 대상이 runtime 값으로 결정되는 경우
- Nuxt auto-import
- Vue global component registration
- webpack/vite plugin이 runtime에 만드는 alias
- reflection 또는 문자열 기반 module loading
- 프로젝트 밖 외부 library 내부 symbol·type 전체 연결
- provider를 다시 실행하지 않는 진짜 file-level incremental 분석

이 부분은 코드만 보고 대상 파일을 확정할 수 없으므로 내부 `CALLS` 또는
내부 `IMPORTS`로 추측하지 않는다. 필요한 경우 framework pack 또는 별도
빌드 메타데이터 adapter의 범위다.

### 29.12 TypeScript·JavaScript 최종 판정

현재 상태:

```text
정적 내부 import resolution: 완료
TypeChecker 기반 호출 range: 완료
TS/JS project unit 분리: 완료
coverage·project-config 구분: 완료
Vue 정적 script/template 연결: 완료
외부 library 경계: 완료
동적 runtime 대상 확정: 미지원
외부 library 내부 semantic 전체 해석: 미지원
```

따라서 TypeScript와 JavaScript는 VisualMap의 “큰 구조 지도” 목적에는 필요한
핵심 기능이 구현된 상태지만, runtime 동작까지 compiler처럼 완전히 확정하는
상태는 아니다.

## 30. 실제 C 대형 프로젝트 검증: cURL

### 검증 대상

`D:\visual_map_reliability_lab\c-curl`을 실제 C 프로젝트로 선택했다. CMake 4.4, Ninja, LLVM-MinGW Clang 22.1.8로 별도 빌드 디렉터리를 만들고 `compile_commands.json`을 생성했다.

정적 라이브러리 설정 기준:

- compile database 항목: 425개
- `lib` 대상 C translation unit: 195개
- 저장소 전체에서 발견된 C/C++ 파일: 755개 수준
- 테스트·문서·플랫폼별 소스 중 일부는 현재 CMake 선택 대상에 포함되지 않음

### 실제 결과

저장소 전체를 한 번에 넣은 결과는 다음과 같다.

- C: 501개 발견, 0개 indexed, 501개 missing
- C++: 254개 발견, 0개 indexed, 254개 missing
- 문서: 0개
- 관계: 0개
- 종료 코드: 0
- 원인: 모든 발견된 translation unit이 현재 compile database에 있어야 한다는 strict 검사에 걸림

`lib`만 넣어도 CMake 정적 빌드 목록에 없는 `dllmain.c` 하나 때문에 같은 방식으로 전체가 중단됐다. 즉 clangd가 임의 결과를 만든 실패가 아니라, 현재 엔진이 “일부 파일이 활성 빌드 대상이 아니면 모듈 전체를 거부”하는 정책 때문에 생긴 결과다.

### 이번 테스트에서 확인한 문제

1. CMake 프로젝트의 저장소 파일 범위와 실제 활성 target 범위가 다르다. 모든 소스 파일이 항상 현재 설정으로 컴파일되는 것은 아니다.
2. CMake 설정이 여러 개면 `compile_commands.json`도 여러 개일 수 있다. 기존 탐색은 첫 번째 파일을 선택하므로, 정적/공유 빌드 중 어느 설정을 쓸지 명확하지 않다.
3. 하위 모듈에서 상위 `build` 디렉터리 안의 중첩 compile database를 찾지 못하는 탐색 누락이 있었다. 상위 빌드 디렉터리와 한 단계 안쪽 설정까지 찾도록 수정했다.
4. build 디렉터리가 source discovery에서 제외되어도 생성된 `compile_commands.json`은 의미 분석 입력이므로 캐시에 포함되어야 한다. 이 파일이 바뀌면 언어·framework·architecture 캐시가 무효화되도록 수정했다.

### 적용한 수정

- `compile_database_dir`가 하위 모듈에서 상위 빌드 디렉터리와 중첩 CMake 설정을 찾도록 보강
- 해당 동작을 검증하는 Rust 단위 테스트 추가
- compile database 내용이 언어 분석 캐시 키에 반영되도록 수정
- Rust 테스트 53개 성공
- release 빌드 성공

### 완성 기준 대조

| 기준 | 현재 판단 | 근거 |
|---|---|---|
| 컴파일 컨텍스트를 확인하고 없으면 추정하지 않음 | 통과 | context 없는 분석을 명시적으로 거부 |
| CMake compile database 탐색 | 부분 통과 | 기본/중첩 탐색은 됐지만 여러 DB 선택 정책이 필요 |
| translation unit 누락 없음 | 미통과 | 활성 target 밖의 파일 하나 때문에 전체 중단 |
| 헤더 중복 방지 | 소규모 통과, 대형 미확정 | 대형 프로젝트가 semantic 단계까지 못 감 |
| C 호출 관계 | 소규모 fixture 통과, cURL 대형 미확정 | 입력 범위 strict 거부 |
| typedef/struct 연결 | 소규모 fixture 통과, cURL 대형 미확정 | 동일 이유 |
| 선언·구현 연결 | 소규모 fixture 통과, cURL 대형 미확정 | 동일 이유 |
| 외부 라이브러리 경계 | 정책 구현됨, 대형 실측 미완료 | semantic 단계 진입 전 차단 |
| provider 오류 파일별 기록 | 부분 통과 | 현재는 모듈 전체 오류로 기록되며 파일별 `not-in-active-build` 구분이 필요 |
| 추정 관계 0개 | 통과 | 실패 시 관계를 만들지 않음 |
| 중복 문서·관계 0개 | 이번 대형 실행에서는 통과 | 문서·관계가 0개였으므로 의미 분석 중복은 아직 실측 필요 |
| 캐시 영향 범위 | 수정 후 재검증 필요 | compile database 변경을 cache key에 포함 |

### 다음에 해결할 핵심

가장 먼저 해야 할 것은 fallback 추가가 아니다. CMake의 활성 target 범위를 엔진이 이해하게 만드는 것이다.

1. compile database에 있는 translation unit만 semantic 분석 대상으로 분리한다.
2. 발견됐지만 현재 target에 없는 파일은 `provider-failed`가 아니라 `not-in-active-build`로 기록한다.
3. 헤더는 활성 translation unit에서 실제로 include된 것과 프로젝트 헤더를 구분한다.
4. 여러 compile database가 있으면 자동으로 조용히 고르지 말고, 활성 target 선택 정보를 결과에 기록한다.
5. 그 뒤 cURL `lib`를 다시 실행해 실제 C 호출·타입·선언-구현 관계를 측정한다.

현재 결론은 C 분석기가 완성됐다는 것이 아니다. 소규모 C/C++ 의미 분석 기준은 통과했지만, 실제 CMake 대형 프로젝트에서는 “저장소 전체 파일”과 “현재 빌드 target 파일”을 분리하는 단계가 아직 부족하다.

## 31. C 활성 target·헤더 문맥·strict gate 개선 결과

30번 테스트에서 확인한 문제를 기준으로 C/C++ provider 실행 범위를 다시 설계했다. 핵심은
저장소에 파일이 있다는 이유만으로 그 파일을 현재 빌드 target의 코드라고 단정하지 않는 것이다.

### 이번에 적용한 변경

1. **활성 translation unit 분리**

   `compile_commands.json`에 실제 컴파일 명령이 있는 C/C++ 소스만 active translation unit으로
   취급한다. 헤더는 소스처럼 독립 컴파일하지 않고, 활성 소스의 문맥에서만 semantic 입력으로
   넘긴다.

2. **여러 compile database 선택**

   프로젝트 아래와 상위 build 디렉터리의 모든 후보를 찾고, 현재 파일 집합과 가장 많이 겹치는
   database를 선택한다. 같은 프로젝트에 static/shared 등 여러 설정이 있어도 첫 번째 파일을
   무조건 고르지 않는다.

3. **헤더용 임시 compile context**

   사용자 프로젝트의 `compile_commands.json`은 수정하지 않는다. provider scratch 디렉터리에
   활성 소스의 컴파일 옵션을 복사한 임시 database를 만들고, 필요한 프로젝트 헤더만 그 문맥으로
   연다.

4. **헤더 소유권과 직접 include 범위**

   C와 C++ provider가 같은 헤더를 중복 문서화하지 않도록 현재 translation unit 언어에 따라
   헤더 소유권을 정한다. 또한 소스에서 직접 include되는 프로젝트 헤더만 semantic 입력으로
   포함한다. 모든 헤더를 독립적으로 열어 생기는 거짓 오류와 중복을 줄였다.

5. **coverage와 strict 정책 분리**

   compile database에 없는 저장소 파일은 `missing`이나 provider 실패가 아니라
   `excluded / not-in-active-build`로 기록한다. 이 경우 `files_missing=0`이면 전체 결과는
   `indexed-partial`로 허용한다. 실제 provider 오류, 누락, 오류 진단은 여전히 strict gate를
   통과하지 못하게 한다.

6. **캐시 무효화 범위 보강**

   한 개의 compile database만 해시하지 않고 프로젝트에서 발견한 모든 compile database의
   경로와 내용을 언어 캐시 키에 포함한다. build profile이 바뀌면 이전 semantic 결과를 재사용하지
   않는다. 캐시 버전은 `v35`로 올렸다.

### 최종 cURL 대형 검증

대상: `D:\visual_map_reliability_lab\c-curl`

```text
종료 코드                 0
최초 실행 시간            111.6초
동일 입력 cache hit        1.2초
발견 파일                 501
indexed                   450
excluded                  51
missing                   0
문서                      450
관계                      9,641
provider error            0
추정 관계                 0
중복 문서                 0
중복 관계                 0
```

동일 입력을 `CODE_MEMORY_STRICT=1`로 재실행한 cache hit도 종료 코드 0으로
확인했다. 즉 strict gate가 캐시 결과에서도 동일하게 적용된다.

51개 파일은 현재 선택된 compile database의 active target 밖이라서 다음처럼 명시적으로
제외됐다.

```text
coverage.status = excluded
coverage.reason = not-in-active-build
```

따라서 이것은 “분석하다가 빠진 누락”이 아니라 “현재 빌드 설정으로 컴파일되지 않는 파일”이다.
사용자가 다른 target을 선택하면 해당 target의 compile database를 기준으로 다시 분석해야 한다.

### 게이트 결과

```text
Rust unit tests                 53 passed
C/C++ completion gate          PASS
C 대형 cURL strict 실행        PASS (exit code 0)
```

### 현재 판정

이번 작업으로 다음 범위는 동작한다.

- compile context 없는 C/C++ 파일에 추정 관계를 만들지 않음
- 여러 compile database 중 현재 파일 집합에 맞는 DB 선택
- active translation unit과 target 밖 파일 구분
- 프로젝트 헤더 문맥 분석 및 C/C++ 중복 방지
- 파일별 coverage 사유 기록
- provider 오류와 target 제외를 strict 정책에서 구분
- compile database 변경 시 캐시 폐기

다음 항목은 아직 “완전 지원”으로 부르면 안 된다.

- CMake/Meson/VCXPROJ 파일 자체를 읽어 target을 재구성하는 parser
- 여러 build profile 중 사용자가 원하는 target을 UI에서 직접 선택하는 기능
- clangd의 모든 경고를 없애는 것. 현재 경고는 원본 프로젝트 코드에 대한 provider 진단이며,
  semantic 실패와 분리해 보존한다.
- C++ template 특수화·복잡한 overload·매크로가 만드는 모든 변형의 완전한 정규화
- 컴파일러 고유 symbol identity를 이용한 모든 cross-translation-unit 선언/구현 병합

즉 현재 결과는 VisualMap의 큰 구조 지도에 필요한 **실제 빌드 범위·문서·호출/타입 관계·제외
사유를 거짓으로 채우지 않고 만드는 단계**까지 안정화된 상태다. compiler와 동일한 완전한 C/C++
의미 해석기까지 완성된 상태는 아니다.

## 32. C/C++ VisualMap 성능 최적화 결과

기능 정확도를 유지한 채 LSP 요청과 반복 I/O를 줄였다.

### 적용 내용

1. C/C++ 함수 본문이 없는 단순 선언에는 호출 계층 요청을 보내지 않는다. 다른 파일에 실제
   구현이 있는 동일 이름 함수는 다시 포함해 선언·구현 연결을 잃지 않도록 했다.
2. 같은 LSP 함수 위치에 대한 `prepareCallHierarchy`와 `outgoingCalls` 결과를 연결 수명 동안
   재사용한다.
3. 동일 프로세스 안에서 compile database 항목 파일 목록과 provider 실행 파일 checksum을
   반복 계산하지 않는다.
4. architecture checksum에서 range를 문자열로 임시 변환하지 않고 정수 바이트로 처리한다.
5. clangd의 verbose command echo와 정보성 stderr를 숨겨 provider 로그 폭증을 막는다.

### cURL cold run 비교

```text
기존 cold run              111.6초
최적화 cold run             97.2초
개선                         14.4초 (약 12.9%)
```

결과 보존 검증:

```text
문서                         450 → 450
관계                       9,641 → 9,641
실제 중복 문서                  0
실제 중복 관계                  0
provider error                 0
추정 관계                      0
strict 종료 코드                0
```

동일 입력 cache hit은 0.9초로 확인했다. clangd verbose 로그도 약 0.78MB에서
수백 바이트 수준으로 줄었다.

### 남은 성능 범위

현재 캐시는 module 단위라서 C/C++ 파일 하나가 바뀌면 해당 module의 semantic provider가
다시 실행될 수 있다. 진짜 target 단위 증분 분석은 compile database 형식만으로는 target
이름과 의존 그래프를 항상 알 수 없으므로, 다음 단계에서 build system별 target metadata 또는
지속형 clangd 세션을 연결해야 한다. VisualMap의 현재 결과 정확도에는 영향을 주지 않으며,
지금은 module cache를 안전한 경계로 유지한다.

## 33. C# VisualMap provider 안정화

### 적용 내용

- 여러 `.sln`/`.slnx`가 있는 저장소에서 파일을 가장 많이 포함하는 solution을 우선 선택
- solution 안의 `.csproj` 경로를 읽어 실제 C# 파일 범위를 기준으로 선택
- `.sln`이 없고 `.csproj`만 있는 프로젝트는 프로젝트 원본을 수정하지 않고 임시 solution 생성
- 임시 solution은 프로젝트와 같은 드라이브에 만들고 provider 실행 후 삭제
- solution·project가 없는 경우에만 명확한 provider 오류로 기록

### 기본 C# semantic 검증

대상: `tests/fixtures/scip-dotnet`

```text
status              indexed
파일                 4 / 4
문서                 4
관계                 15
CALLS                3
IMPLEMENTATION       3
오류                 0
추정 관계            0
```

### `.csproj` only fallback 검증

임시 2파일 C# 프로젝트로 solution 자동 생성을 검증했다.

```text
status              indexed
파일                 2 / 2
문서                 2
관계                 5
CALLS                1
IMPLEMENTATION       1
생성 solution        provider 실행 후 삭제
strict 종료 코드     0
```

### C# framework pack 검증

```text
ASP.NET Core          PASS
ASP.NET MVC           PASS
ASP.NET Web API       PASS
Minimal API           PASS
Blazor                PASS
.NET MAUI             PASS
총 6 / 6              PASS
```

### 실제 GodotTools 검증 결과

`D:\visual_map_reliability_lab\cpp-godot\modules\mono\editor\GodotTools`는
소스 61개·프로젝트 8개의 실제 C# solution으로 실행했다. solution 선택과 restore까지는
정상 동작했지만, 저장소의 `Directory.Build.props`가 요구하는
`modules/mono/SdkPackageVersions.props` 파일이 실제 checkout에 없어 MSBuild가 모든
프로젝트를 열지 못했다.

```text
provider 실행       완료
문서                 0
원인                 외부 프로젝트 빌드 설정 파일 누락
엔진 오류            아님
```

따라서 현재 C# 판정은 **VisualMap 목적의 일반 C# project/solution 분석과 framework
pack은 통과**이며, GodotTools처럼 저장소 외부 생성 파일에 의존하는 프로젝트는 해당
프로젝트를 먼저 정상 build 가능한 상태로 만들어야 한다.

## 34. C# provider 실행 비용 최적화

### 문제

C# provider를 실행할 때 엔진이 이미 수집한 C# 파일을 다시 전체 순회해 solution을
선택했다. 또한 소스 파일만 변경되어도 `scip-dotnet`이 매번 `dotnet restore`를 실행했다.
대규모 프로젝트에서는 provider 자체 분석보다 restore와 중복 탐색이 불필요한 대기시간을
만들 수 있다.

### 해결

- 엔진이 이미 수집한 C# 파일 목록을 solution 선택 로직에 재사용한다.
- 프로젝트 설정 체크섬과 선택된 solution 경로를
  `%LOCALAPPDATA%\VisualMap\cache\code-memory\<project>\dotnet-restore-state`에 저장한다.
- 소스만 바뀌고 프로젝트 설정 체크섬이 같으면 `--skip-dotnet-restore`를 사용한다.
- `.csproj`, `.sln`, `.props`, `.targets` 등 프로젝트 설정이 바뀌면 체크섬이 달라져
  자동으로 일반 restore로 돌아간다.
- provider 실행 또는 SCIP 출력 생성이 실패하면 restore 상태를 저장하지 않는다.

### 검증

동일한 4파일 C# fixture에서 다음을 확인했다.

```text
첫 실행(restore)             3.38초 / indexed / 문서 4 / 관계 15
소스만 변경(skip restore)     2.12초 / indexed / 문서 4 / 관계 15
프로젝트 설정 변경(restore)  3.68초 / indexed / 문서 4 / 관계 15
오류                         0
```

이번 변경은 분석 결과를 줄이거나 추정 관계를 추가하지 않는다. restore를 건너뛰는 조건은
프로젝트 설정 체크섬이 동일할 때로 제한했다.

## 35. TypeScript/JavaScript project model cache와 C/C++ 대형 gate 보정

### TypeScript/JavaScript project model cache

언어 결과 cache가 있어도 기존에는 매 실행마다 bundled TypeScript compiler API로 전체
project model을 다시 만들었다. 이제 프로젝트 root, TypeScript/JavaScript/Vue 파일 hash,
프로젝트 설정 checksum, Node provider와 manifest를 묶은 key를 사용한다.

```text
첫 실행       indexed / 문서 2 / 관계 6
두 번째 실행  indexed / 문서 2 / 관계 6 / project model cache hit
두 번째 실행 시간 162ms
오류          0
```

소스·설정·provider가 바뀌면 key가 달라져 자동으로 다시 계산한다. cache는 결과 정확도를
낮추는 fallback이 아니라 동일한 compiler 결과를 재사용하는 용도다.

### C/C++ 대형 gate 보정

cURL 재검증에서 엔진은 다음을 정확히 반환했다.

```text
발견 501 / indexed 450 / excluded 51 / missing 0
추정 관계 0 / 오류 진단 0
```

51개는 현재 compile database의 active target 밖이라 `not-in-active-build`로 제외됐다.
기존 gate가 `indexed`만 허용해 정상적인 `indexed-partial` 결과를 실패 처리하던 문제를
수정했다. 이제 gate는 indexed/excluded를 모두 합쳐 전체 파일이 설명됐는지와 오류·누락이
없는지를 검증한다.

### 추가 provider/gate 검증

- PHP provider에 프로젝트가 Composer autoload에 직접 등록하지 않은 PHP 파일을 임시
  include 목록으로 전달하도록 보강했다. 사용자 `composer.json`은 수정하지 않는다.
- 이 방식으로 CodeIgniter의 `app/Config/Routes.php`도 분석되어 PHP framework gate가
  `indexed-partial`에서 `indexed`로 바뀌었다.
- 선언된 Rust Tauri pack의 fixture 생성기가 빠져 있던 문제를 추가했다.

```text
PHP framework provider gate       7 / 7 PASS
전체 framework provider gate     84 / 84 PASS
전체 언어 semantic gate          12 / 12 PASS
C/C++ completion gate             PASS
C/C++ cURL 대형 gate              PASS
meeting-overlay source gate      PASS
```

meeting-overlay 실제 비교 결과:

```text
server routes       53 / 53
React components    28 / 28
Tauri JS invokes     4
Tauri Rust commands  4
source comparison   PASS
```

## 36. 대형 LSP 부분 결과와 언어별 실프로젝트 재검증

이번 재검증에서 provider가 일부 파일만 반환했는데 최종 언어 요약이 `indexed`로
표시되는 공통 집계 오류를 발견했다. `merge_language_analyses`가 모듈별 결과를
합칠 때 `files_missing`을 상태에 반영하지 않았고, 빈 semantic 결과를 `excluded`로
분류하고 있었다.

수정 후 규칙은 다음과 같다.

- provider가 반환하지 않은 파일은 `missing`으로 기록한다.
- provider가 의미 정보를 하나도 반환하지 않은 모듈도 `missing`으로 기록한다.
- 프로젝트 설정이나 활성 C/C++ 빌드 밖 파일처럼 의도적으로 제외된 경우만 `excluded`다.
- `files_missing > 0`이면 최종 언어 상태는 `indexed-partial`이다.
- strict gate는 부분 결과를 성공으로 통과시키지 않는다.

실제 결과:

```text
go-prometheus: 726 found / 14 indexed / 712 missing / indexed-partial
java-spring-petclinic-microservices: 53 found / 43 indexed / 10 missing / indexed-partial
matomo: 2465 PHP indexed, 223 TypeScript indexed, 99 JavaScript indexed
ruby-redmine: 717 found / 689 indexed / 28 missing / indexed-partial
```

Rust workspace는 중첩 `Cargo.toml`마다 rust-analyzer를 새로 실행하지 않고 루트
`Cargo.toml`에 `[workspace]`가 있으면 하나의 workspace provider로 처리하도록 바꿨다.
또한 500개를 넘는 LSP workspace에서는 문서 심볼과 import 구조를 유지하고, 파일마다
반복되는 references/type/call-hierarchy 질의는 생략한다. 이는 추정 관계를 추가하는
fallback이 아니며, 생략 사실은 warning으로 남는다.

이 정책으로 Ruby Redmine은 기존 session timeout에서 약 21초 provider 처리로 개선됐고,
689개 문서와 28개 누락을 반환했다. 반면 Nushell은 rust-analyzer가 workspace 세션
예산 안에 응답하지 않아 `indexer-failed`로 남았다. Dart DevTools도 `.dart_tool`
package config가 없는 상태에서 Dart analysis server가 장시간 응답하지 않아 결과를
완료로 표시하지 않았다. 두 경우 모두 관계를 추정해 채우지 않는다.

회귀 검증:

```text
cargo test: 58 passed
language semantic gate: 12 / 12 passed
```

## 37. 남은 언어 공통 정확도 보정과 회귀 검증

### Python `from package import submodule`

기존 lexical import parser는 다음 문장을 `app` 패키지까지만 저장했다.

```python
from app import service
```

그 결과 `app/service.py`가 있어도 `app.py` 또는 `app/__init__.py`만 후보가 되어
실제 모듈 파일 연결이 빠질 수 있었다. import 문에 적힌 member를 보존하고,
`app/service.py` 또는 `app/service/__init__.py`가 실제로 하나만 존재할 때만
내부 `IMPORTS` 관계를 만든다. 파일이 없거나 여러 후보면 추정하지 않고 외부/미해결
경계로 남긴다.

### Rust `use crate::...`

Rust `use`를 첫 경로 조각만 저장하던 문제를 수정했다. 이제 `crate::service::run`,
`self::module`, `super::module`, 중괄호 import의 실제 모듈 경로를 유지하고,
프로젝트의 `.rs` 또는 `mod.rs`가 확인될 때만 내부 import 관계를 만든다.

### PHP namespace와 Composer식 경로

PHP namespace/class 선언을 분석 시작 시 한 번 인덱싱한다. `use App\\Service\\UserService`
와 프로젝트 파일의 `namespace App\\Service; class UserService`가 유일하게 일치할 때만
내부 파일로 연결한다. 중복 선언은 인덱스에서 제거하여 임의 대상을 선택하지 않는다.
이전의 import마다 PHP 전체 파일을 다시 순회하던 구조를 없애 내부 경로 조회를
`O(1)`에 가깝게 줄였다.

### Java Maven 상위 reactor

하위 모듈에서 분석을 시작해도 상위 `pom.xml`의 `<modules>`가 확인되면 상위 reactor를
LSP workspace root로 사용한다. 하위 모듈마다 JDTLS를 중복 기동하지 않는다.

### Gradle/Python project metadata

`setup.cfg`, `setup.py`, `build.gradle`, `build.gradle.kts`의 실제 name/version만
package tree metadata로 읽는다. dependency를 설치하거나 코드를 추정하지 않는다.

### Dart dependency preflight

`workspace`, Flutter, `dependencies`, `dev_dependencies`, `dependency_overrides`가
있는 Dart project에서 `.dart_tool/package_config.json`이 없으면 analysis server를
성공으로 표시하지 않는다. dependency 설치는 수행하지 않고 원인을 `indexer-failed`로
기록한다.

### 최신 회귀 검증

```text
Rust release tests                 68 passed
Framework pack gate                84 / 84 passed
Language semantic fixture gate     12 / 12 passed
Language cache marker              v66
```

위 semantic gate는 작은 fixture 기준이다. 대형 workspace에서 per-symbol LSP 질의를
제한하는 정책은 그대로 유지되며, 생략 사실은 warning으로 남는다. 따라서 fixture gate
통과만으로 대형 프로젝트의 모든 호출 관계가 완전하다고 판정하지 않는다.

## 38. 남은 언어 실프로젝트 재검증 결과

대형 provider의 기본 180초 예산에서 Go Prometheus가 중단되어 726개 중 14개만 반환된
`indexed-partial` 결과를 확인했다. 이는 성공으로 처리하지 않았다. 900초 예산으로
재실행한 결과는 다음과 같다.

| 프로젝트 | 언어 coverage | 문서 | 관계 | provider | 판정 |
|---|---:|---:|---:|---:|---|
| Spring Petclinic Microservices | Java 53/53, JS 22/22 | 75 | 205 | 19.2초 | 통과 |
| Prometheus | Go 725/726, 1 build 제외, 누락 0 | 971 | 18,975 | 303.1초 | 정확도 통과, 성능 개선 필요 |
| Nushell | Rust 1,465/1,465, 누락 0 | 1,467 | 76 | 11.5초 | coverage 통과, 대형 semantic 제한 경고 |
| Matomo | PHP 2,465/2,465, TS 223/223, JS 99/99 | 2,787 | 108,964 | 58.6초 | 통과 |
| Redmine | Ruby 717/717, JS 128/128, Python 1/1 | 846 | 161,579 | 13.4초 | 통과 |
| Dart DevTools | Dart 696/696 누락 | 1 | 70 | 0.8초 | dependency metadata 부족으로 의도적 실패 |

Prometheus의 1개 제외는 Go build constraint 파일이며 누락이 아니다. Rust Nushell은
coverage와 import 구조는 완성됐지만 500개 초과 workspace 정책으로 per-symbol
references/type/call-hierarchy 질의를 생략했다. Dart는 프로젝트의
`.dart_tool/package_config.json`이 없는 상태라 dependency를 설치하지 않고 실패로
기록했다.

## 39. 대형 Go LSP 성능 실험과 세션 예산

Prometheus에서 대형 workspace의 파일별 `documentSymbol` 요청을 줄이기 위해 빈
`workspace/symbol` 한 번으로 대체하는 실험을 했다. gopls는 이 질의 자체에서 전체
workspace를 다시 구성해 기존 약 303초보다 오래 실행됐고, 7분 이상 CPU를 사용했다.
정확도와 성능 모두에 이득이 없어 해당 변경은 되돌렸다.

현재 정책은 다음과 같다.

- provider가 실제로 반환한 symbol/call만 저장한다.
- 대형 workspace에서 세부 질의를 생략하는 경우 warning을 남긴다.
- 대형 LSP workspace의 기본 세션 예산은 900초로 늘려 180초 조기 중단을 막는다.
- 일반 프로젝트의 응답 timeout은 기존 30초를 유지한다.
- provider가 예산 안에 끝나지 않으면 성공으로 위장하지 않고 `indexer-failed` 또는
  `indexed-partial`로 기록한다.

따라서 Go Prometheus는 현재 정확도는 통과하지만 cold provider 시간이 약 5분이며,
다음 성능 개선은 Rust 후처리가 아니라 gopls 자체의 workspace 초기화 비용을 줄일 수
있는 provider 실행 전략을 별도로 검토해야 한다.

Rust fixture에서 symbol 응답 직후 call hierarchy가 준비되지 않아 `CALLS`가 간헐적으로
비어 있던 문제도 확인했다. Rust LSP workspace 시작 대기 시간을 1.5초에서 5초로
늘리고 cache marker를 `v65`로 갱신한 뒤 fixture에서 `main → add` CALLS가 안정적으로
생성됐다.

최신 회귀 결과:

```text
Rust release tests             68 passed
Framework pack gate            84 / 84 passed
Language semantic gate         12 / 12 passed
```

## 40. 대형 Go `didOpen` 중복 전송 제거

Go Prometheus의 303초 병목을 추가 분석한 결과, gopls가 디스크의 workspace를 이미
읽는데 엔진이 711개 파일의 전체 내용을 다시 `textDocument/didOpen`으로 전송하고
있었다. CLI 정적 분석에는 unsaved editor buffer가 없으므로 대형 `gopls` workspace에
한해 이 중복 전송을 건너뛰고, 파일은 디스크에서 직접 읽도록 변경했다.

변경 전후 결과:

| 항목 | 변경 전 | 변경 후 |
|---|---:|---:|
| provider 처리 시간 | 303.1초 | 18.3초 |
| Go 발견 파일 | 726 | 726 |
| Go indexed | 725 | 725 |
| Go 제외 | 1 | 1 |
| Go 누락 | 0 | 0 |
| 문서 | 971 | 971 |
| 관계 | 18,975 | 18,975 |
| framework 관계 | 595 | 595 |

결과와 관계를 그대로 유지하면서 provider 입력 중복만 제거한 최적화다. 일반 LSP와
편집 중인 문서가 필요한 경로에는 적용하지 않았다.

## 41. 대규모 LSP의 VisualMap 경계 질의

500개를 넘는 LSP workspace에서 모든 심볼에 call hierarchy, type hierarchy,
references 질의를 보내면 gopls·Dart Analysis Server·Ruby LSP가 workspace를
반복 구성하면서 실행 시간이 급증했다. 기존 구현은 Rust 외 언어의 질의를 전부
생략했기 때문에 파일은 indexed여도 VisualMap의 호출 흐름이 약해지는 문제가 있었다.

변경 내용:

- provider가 실제로 반환한 공개·모듈 경계 심볼에만 call/type 질의를 제한적으로 허용
- 이름을 보고 CALLS 대상을 만드는 fallback은 추가하지 않음
- private/non-boundary 심볼의 reference와 lexical 질의는 계속 생략
- Go는 exported receiver method 질의가 gopls workspace를 다시 구성해 partial 결과를
  만들 수 있어 large map enrichment 대상에서 제외하고 기존 안정 경로 유지
- 대규모 결과에는 생략 범위를 warning으로 남김

Python meeting-overlay-assistant 재검증에서는 613/613 파일이 유지되고 CALLS가
2,011개에서 2,720개로, REFERENCES가 4,287개에서 4,804개로 늘었다. 이 결과는
provider가 확인한 관계만 포함하며, 설치되지 않은 fastapi·dotenv·psycopg 등의
외부 의존성은 unresolved 진단과 외부 경계로 남는다.

## 42. Windows LSP file URI 대소문자 오류

Windows 경로를 `file:///D:/...` 형태로 보내던 중 rust-analyzer가 응답 위치를
`file:///d:/...`로 반환하면서 문서가 서로 다른 파일로 취급됐다. 결과적으로 Rust
fixture가 문서 0개인 `empty-semantic`으로 끝나는 현상이 있었다.

`path_to_uri`에서 Windows 드라이브 문자를 소문자로 정규화하고 회귀 테스트를 추가했다.
수정 후 Rust fixture는 문서 2개, 관계 3개로 통과했고 전체 언어 gate도 12/12를
통과했다.

## 43. 대형 source snapshot의 불필요한 원문 읽기

architecture 단계가 provider가 이미 `provider-size-limit`으로 제외한 1MB 초과
파일까지 전부 읽고 해시했다. Dart DevTools에는 test-data와 compiled data를 포함해
약 255MB의 큰 소스 파일이 있어 초기 분석과 architecture cache key 계산이 크게
느려졌다.

변경 내용:

- 1MB 초과 파일은 경로를 snapshot에 보존하되 원문은 빈 문자열로 보존
- 파일 크기와 수정 시간으로 fingerprint 생성
- architecture/framework cache key는 이미 계산한 `file_hashes`를 정렬해 사용
- 전체 소스 문자열을 cache key 계산에서 다시 해시하지 않음

따라서 파일 트리는 유지하면서 대형 데이터 파일의 중복 읽기와 해시를 제거했다.

## 44. Dart import resolver의 전체 경로 반복 검색

Dart `package:`와 상대 import가 해석되지 않을 때마다 전체 source path를 suffix
검색하던 문제가 있었다. 외부 package가 많을수록 import 수 × 전체 파일 수에 가까운
비용이 발생했다.

변경 내용:

- local `package:` import는 계산된 package root의 exact path만 확인
- `dart:` SDK import는 프로젝트 파일 검색을 하지 않음
- Dart 상대 import도 exact path만 확인
- 찾지 못하면 외부/미해결로 두고 내부 관계를 만들지 않음

이 정책은 정확도를 낮추는 fallback이 아니라, 확인할 수 없는 외부 대상을 내부 코드로
잘못 연결하지 않는 정밀도 우선 정책이다.

## 45. 최신 Dart DevTools 재검증

`D:\visual_map_reliability_lab\dart-flutter-devtools`에서 다음 결과를 확인했다.

| 항목 | 결과 |
|---|---:|
| Dart coverage | 696/696 indexed |
| 전체 문서 | 697 |
| 전체 관계 | 8,547 |
| provider 처리 | 약 40초 |
| architecture 처리 | 약 0.6초 |
| 프로젝트 수정 | 없음 |

Dart는 provider와 후처리 성능은 개선됐지만, 외부 Flutter package가 없는 환경에서는
missing package 진단이 남는다. `package_config.json`이 없을 때 dependency를
다운로드하지 않고 local-only synthetic map을 사용하는 현재 정책은 유지한다.
Serverpod 대형 workspace는 dependency metadata가 없는 경우 provider를 강제로
기다리지 않고 `empty-semantic`과 architecture-only 결과로 분리한다. 자세한 내용은
아래 47번을 따른다.

## 46. Go build-tag 전용 모듈의 빈 semantic 결과

Prometheus에는 `//go:build` 조건부 파일만 들어 있는 Go 모듈이 있었다. 이 파일들을
gopls에 그대로 넘기면 provider가 semantic fact 없이 종료했는데, 다른 모듈의 성공
결과와 합쳐질 때 전체 Go 상태가 `indexed`로 보일 수 있었다.

변경 내용:

- source exclusion 정책에 해당하는 파일은 provider 입력에서 제거
- 원래 coverage에는 파일을 남겨 `go-build-constraint` 이유를 기록
- 해당 파일만 있는 모듈은 provider를 실행하지 않음
- 실제 누락과 조건부 제외를 분리

최신 Prometheus 재검증은 Go 726개 중 648개 indexed, 78개 `go-build-constraint`
excluded, missing 0이었다. 따라서 78개는 provider 누락이 아니라 현재 build
context 밖의 조건부 소스이며, 이름 기반 관계나 임의 fallback은 추가하지 않았다.

## 47. Dart 대형 workspace와 dependency metadata 없는 상태

Dart/Flutter는 `.dart_tool/package_config.json`이 없으면 analysis server가 외부
package와 workspace를 해석하지 못한다. 특히 Melos 프로젝트에서 100개가 넘는
package를 synthetic map으로 돌리면 analysis server가 per-symbol 질의 중 장시간
대기할 수 있었다.

변경 내용:

- Dart package를 최대 512개 파일 단위 provider 작업으로 분리하되 package root는
  유지
- Dart provider job은 같은 workspace에서 동시에 실행하지 않음
- synthetic package map에서는 공개 심볼별 call/type 질의를 생략하고 선언·import·파일
  흐름만 유지
- Melos workspace에 resolved `.dart_tool/package_config.json`이 없으면 provider를
  무리하게 실행하지 않고 `empty-semantic`과 dependency gap 진단을 기록
- architecture는 provider가 없어도 source snapshot으로 파일·모듈·import 지도를 생성

검증 결과:

- Dart DevTools: 696개 중 692개 indexed, 4개 `provider-size-limit` excluded,
  missing 0, provider 약 23초
- Serverpod: provider merge 약 0.6초, Dart 891개는 dependency gap으로 명시,
  architecture 7,365 nodes / 10,146 edges 생성

실제 `.dart_tool/package_config.json`이 있는 사용자 프로젝트에서는 이 제한을
적용하지 않고 Dart analysis server의 semantic 결과를 사용한다.

## 48. Rust 문서 심볼 조기 확정과 빈 semantic cache 재사용

작은 Rust fixture에서 provider 상태는 `indexed`였지만 CALLS 관계가 0개로 남는
문제가 있었다. rust-analyzer가 초기 응답에서 모듈 껍데기만 먼저 반환했는데,
기존 코드는 문서 심볼이 하나라도 있으면 응답을 확정해 함수 심볼과 호출 질의를
놓쳤다. 과거의 빈 semantic 결과가 cache에 남아 있으면 provider를 다시 실행하지
않는 문제도 함께 확인됐다.

변경 내용:

- Rust 문서 심볼은 callable 심볼이 확인될 때까지 기존 재시도 범위에서 대기
- semantic cache loader는 문서가 비어 있는 캐시를 재사용하지 않음
- 언어 cache marker를 `v90`에서 `v91`로 올려 기존 결과를 전체 무효화

검증 결과:

- Rust fixture: 2/2 indexed, 관계 3개, CALLS 확인, diagnostics 0
- 전체 언어 semantic gate: 12/12
- framework pack gate: 84/84
- framework semantic self-test: 84/84
- framework provider gate: 84/84

## 49. C/C++ 대형 compile database의 헤더 coverage와 clangd 처리 시간

curl 대형 프로젝트를 C/C++ scale gate로 실행할 때 처음에는 clangd가 약 130초
후 timeout되어 문서 0개를 반환했다. clangd 대형 모드와 reachable header 처리를
정리한 최신 실행에서는 450개 module files를 처리하고 문서 377개, 관계 1,455개를
만들었으며 provider 처리 시간은 약 204초였다.

compile database에 포함된 파일 중 `Makefile.inc`와 `.rc`처럼 compiler
invocation을 만들 수 없는 항목은 clangd 경고로 남는다. 또한 활성 translation
unit에서 include되지 않는 헤더는 standalone semantic 문서로 만들 수 없으므로
missing으로 표시하면 실제 코드 누락처럼 보인다.

변경 내용:

- 활성 translation unit에서 실제로 도달 가능한 project header만 semantic 대상에
  포함
- 도달하지 않는 고립 헤더는 source/architecture에는 남기고
  `header-not-reachable` excluded로 분류
- 실제 translation unit 누락만 missing으로 유지
- C/C++ completion gate에서 문서·coverage·관계 중복 검증을 유지

검증 결과:

- C/C++ completion gate 통과
- 전체 언어 semantic gate 12/12 통과
- curl scale 실행 결과 C language는 377 indexed, 73 `header-not-reachable`
  excluded, missing 0이다. 별도로 build context 밖 파일 51개는
  `not-in-active-build`로 제외된다. 언어 상태의 partial 표시는 이 build 제외를
  포함한 것이며, active translation unit과 reachable header에는 missing이 없다.
- scale gate는 `active files >= 250`을 large-map 입력으로 판단한다. clangd 대형
  모드의 실제 기준과 맞추기 위해 fixture 조건을 정정했으며, curl의 450개 active
  module files는 이 조건을 충족한다. 최신 실행은 약 204초가 걸렸다.

## 50. Dart synthetic package map의 실사용 한계

설치나 네트워크 없이 local-only Dart semantic 결과를 얻기 위해 Melos 프로젝트에
synthetic `package_config`를 만들어 analysis server를 실행하는 opt-in 실험을
Serverpod에서 수행했다. analysis server가 package graph를 완료하지 못한 채 여러
chunk를 순차적으로 오래 대기했고, 4분이 지나도 semantic output을 만들지 않았다.

따라서 synthetic map을 기본 semantic 경로로 승격하지 않았다. 현재 정책은 다음과
같다.

- 실제 `.dart_tool/package_config.json`이 있으면 Dart analysis server semantic 사용
- 없으면 dependency 설치·네트워크 없이 architecture/source map 생성
- 외부 package는 외부 경계와 dependency gap으로 표시
- 무기한 provider 대기나 추정된 Dart 호출 관계는 만들지 않음

## 51. architecture import 해석의 반복 경로 검색과 source 문자열 복사

대형 프로젝트에서 architecture 후처리가 import마다 전체 source path를 다시
순회하는 경로가 남아 있었다. Python/Ruby/Java/C 헤더 등의 후보가 정확히 하나인지
확인할 때 `import 수 × source 파일 수`에 가까운 비용이 발생할 수 있었다. 또한
source boundary를 만들 때 전체 경로 목록과 각 source 문자열을 임시로 복사했다.

변경 내용:

- `SourcePathIndex`를 architecture builder 생성 시 한 번 만든다.
- exact path와 path suffix의 후보를 인덱싱하고, 후보가 2개 이상이면 그대로
  ambiguous/external로 남긴다. 이름 기반 대체 관계는 만들지 않는다.
- C/C++ include, Python/Rust/Java/C#/Ruby/Dart import와 Go module directory
  조회가 이 인덱스를 사용한다.
- source boundary는 `source_texts`를 빌려 읽어 전체 문자열 복사를 제거한다.

복잡도는 일반적인 import 해석을 `O(import 수 × source 파일 수)`에서
`O(source 파일 수 × 경로 깊이 + import 수 × 후보 수)`로 줄였다. 후보 수는
언어별로 일정한 작은 값이며, ambiguity 판정은 유지된다.

검증:

- Rust release tests: 76/76 passed
- release build: 성공

## 52. 혼합 언어 프로젝트의 Rust provider 동시 실행 충돌

`meeting-overlay-assistant` 전체 루트를 한 번에 분석할 때 Rust 파일만
`empty-semantic`으로 끝나는 현상이 있었다. Rust 디렉터리만 분리해 실행하면
정상 결과가 나왔으므로 Rust 문법 분석 자체가 아니라 JavaScript/Python provider와
동시에 같은 저장소의 workspace 상태를 읽는 `rust-analyzer` 재로드 경쟁으로
판단했다.

변경 내용:

- Rust provider job을 weight 4로 지정해 기본 provider weight 한도에서 단독으로
  실행한다.
- 다른 언어의 동시 처리는 유지하고, Rust workspace 세션만 격리한다.
- 빈 semantic 결과를 성공으로 숨기지 않고 기존 `empty-semantic` 상태와 진단을
  유지한다.

검증:

- 패치 전 전체 루트: Rust 0/2, `empty-semantic`
- 패치 후 전체 루트: Rust 2/2, `indexed`
- meeting-overlay source comparison: 통과

## 53. C# 솔루션 밖 파일의 잘못된 missing 분류와 모듈별 언어 캐시 충돌

NopCommerce의 `src/Build/src/ClearPluginAssemblies/Program.cs`는 저장소 안의
C# 파일이지만 선택된 `NopCommerce.sln` 프로젝트에 포함되지 않았다. 기존에는
provider가 반환하지 않은 파일로만 분류되어 실제 분석 누락처럼 보였다.

변경 내용:

- 선택된 `.sln/.slnx`의 `.csproj` 경계를 추출한다.
- C# 파일이 솔루션 프로젝트 경계 밖이면 `project-config` excluded로 기록한다.
- 솔루션 밖 파일을 C# `missing`과 `indexed-partial`의 원인으로 세지 않는다.
- 언어 캐시 파일명을 언어명 하나가 아니라 `언어명 + 모듈 cache key`로 만든다.
  따라서 같은 루트에서 Dart chunk나 여러 하위 모듈이 서로의 캐시를 덮어쓰지
  않는다.

검증:

- NopCommerce C#: 3,613 indexed, 1 project-config excluded, missing 0,
  language status `indexed`
- Rust release tests: 76/76 passed
- release build: 성공

## 54. 대형 Go workspace에서 gopls 파일 열기 생략으로 발생한 coverage 누락

Prometheus를 실제로 분석했을 때 Go 726개 중 14개만 provider 문서가 되고
634개가 `not-returned-by-provider`로 남았다. 원인은 대형 workspace에서
`gopls`가 디스크 파일을 이미 읽는다고 가정하고 `didOpen`을 생략한 최적화였다.
이 환경에서는 `documentSymbol`이 열린 문서와 workspace 상태에 의존해 전체
파일 문서를 반환하지 않았다.

변경 내용:

- Go 대형 workspace에서도 분석 대상 파일을 provider에 열도록 수정
- 기존 대형 모드의 per-symbol 호출·reference 확장 제한은 유지
- coverage가 회복되는 대신 provider 입력과 처리 시간이 늘어나는 trade-off를
  선택했다. VisualMap의 지도 누락 방지가 속도보다 우선이다.

검증:

- 패치 전 Prometheus: Go 14 indexed, 634 missing, 78 build-constraint excluded
- 패치 후 Prometheus: Go 648 indexed, 78 build-constraint excluded, missing 0,
  status `indexed`
- 패치 후 release build 성공

## 55. 대형 Rust의 VisualMap용 호출 범위와 성능 한계

Nushell은 Rust 1,465개 파일을 모두 provider 문서로 만들었지만, 공개 메서드마다
call hierarchy를 질의하면 지도에 필요 이상의 세부 관계가 생기고 provider 시간이
길어졌다. Rust 대형 모드에서는 최상위 공개 API와 타입 경계만 semantic enrichment
대상으로 두고, 1,000개 초과 프로젝트의 call hierarchy는 생략한다.

이 정책은 이름으로 대상을 추정하거나 관계를 만들어내지 않는다. 모든 문서·선언은
provider 결과로 유지하고, 생략된 call hierarchy만 명시적인 warning으로 기록한다.

검증:

- Rust 1,465/1,465 indexed, missing 0
- call hierarchy 범위 축소 전: 8분 28초, relations 5,732
- 최상위 공개 API 정책 후: 6분 59초, relations 76
- Rust provider warning/error가 아닌 외부 crate 진단은 별도 warning으로 보존

`workspace/symbol` 전체 대체 실험은 6분 37초로 추가 개선이 22초뿐이었고,
연결되지 않은 파일 warning을 1,689개 추가했기 때문에 채택하지 않았다.

## 56. 대형 PHP provider 검증

Matomo를 실제로 분석해 PHP provider의 외부 라이브러리와 대형 파일 coverage를
확인했다. `vendor`, `node_modules`, build 산출물 등 source policy 제외 경로는
분석 대상에서 제외하고, 실제 프로젝트 PHP 파일은 provider 결과로 검증했다.

검증 결과:

- PHP 2,465/2,465 indexed
- missing 0, excluded 0
- provider merge 약 71초
- PHP provider의 PHP 8 호환성 deprecation 출력은 warning으로만 남았고
  indexing 실패로 승격되지 않음

## 57. strict quality gate 최종 검증

기본 모드는 부분 결과를 시각화할 수 있도록 유지하고, 배포 전 검증에서는
`CODE_MEMORY_STRICT=1`을 사용한다. strict 모드는 언어가 `indexed`가 아니거나
error 진단이 있으면 종료 코드 1을 반환한다. 단, missing 없이 명시적으로
excluded된 `indexed-partial`은 허용한다.

최종 검증:

- `CODE_MEMORY_STRICT=1` + 12개 언어 semantic gate: 12/12 통과
- framework provider gate: 84/84 통과
- 실제 외부 프로젝트 source comparison: 통과

현재 캐시는 source hash와 provider/config 입력을 포함한 모듈 단위 cache key를
사용한다. 같은 프로젝트 안의 여러 chunk/module이 서로 덮어쓰지 않도록 캐시 파일도
`language + module key`로 분리했다. provider를 재실행하지 않는 완전한 파일 단위
증분은 언어 workspace 의미 보존 문제 때문에 아직 별도 작업 범위다.

## 58. native LSP source 문자열 중복 복사

LSP provider는 파일을 읽은 뒤 source cache에 문자열을 저장하면서 같은 내용을
`didOpen`용으로 한 번 더 복사하고 있었다. 대형 프로젝트에서는 파일 수와 파일
크기에 비례해 메모리 사용량이 커질 수 있다.

변경 내용:

- 읽은 문자열을 source cache로 이동한다.
- `didOpen`은 cache에서 빌린 문자열을 사용한다.
- source 내용의 불필요한 전체 복사 1회를 제거한다.

검증:

- Rust release tests: 76/76 passed
- release build: 성공

## 59. framework·architecture cache key의 중복 파일 읽기

### 문제

`SourceSnapshot`이 이미 모든 분석 대상 소스의 checksum을 보유하고 있고
프로젝트 설정 digest도 index 시작 시 한 번 계산한다. 그런데 framework cache
key가 문서마다 원본 파일을 다시 읽었고, architecture cache key도 설정 파일을
다시 순회했다. 대형 프로젝트에서는 semantic 결과를 바꾸지 않는 중복 디스크
읽기가 추가됐다.

### 해결

- framework cache key는 문서 파일을 다시 읽지 않고 `SourceSnapshot` checksum을
  사용한다.
- framework·architecture cache key는 이미 계산한 `project_config_digest`를
  전달받아 설정 파일 재순회를 없앴다.
- source/config/provider 입력 자체는 기존처럼 checksum에 포함되므로 cache
  무효화 기준은 유지된다.

### 검증

- `cargo fmt --manifest-path rust/Cargo.toml -- --check`: 통과
- Rust release tests: 76/76 통과
- release build: 성공
- `run-external-project-gate.ps1 -ProjectRoot D:\meeting-overlay-assistant`:
  server routes 53/53, React components 28, Tauri JS invokes 4, Tauri Rust
  commands 4, source comparison 통과
- 동일 server-routes 입력의 cache hit: exit 0, 약 131ms

### 남은 경계

현재 provider cache는 파일 checksum을 포함한 **모듈 단위**다. 파일 하나만
바뀌었을 때 같은 workspace의 provider를 재실행하지 않고 심볼 일부만 갱신하는
진짜 file-level incremental은 아직 구현하지 않았다. 언어 provider가 가진
cross-file type/import 의미를 보존하려면 변경 파일의 dependency closure와
provider별 incremental protocol이 필요하므로, 단순히 파일 결과를 합치는
fallback은 추가하지 않는다.

## 60. 오프라인 Rust 외부 crate 로딩 중 rust-analyzer SendError panic

### 문제

외부 프로젝트의 Rust `Cargo.toml`에 `tauri`, `serde` 같은 crate가 있지만
분석용 providers 폴더에는 프로젝트 dependency를 내려받지 않는다. 이 상태에서
rust-analyzer가 Cargo metadata, build script, proc-macro, sysroot를 계속
로드하다가 종료 시점에 내부 `SendError(..)` panic을 stderr에 출력했다. 결과가
생성되더라도 provider panic을 정상 상태로 취급할 수 없는 문제다.

### 해결

Rust LSP workspace 설정에 다음을 적용했다.

- `rust-analyzer.cargo.noDeps = true`
- `rust-analyzer.cargo.allTargets = false`
- `rust-analyzer.cargo.autoreload = false`
- `rust-analyzer.cargo.buildScripts.enable = false`
- `rust-analyzer.procMacro.enable = false`

프로젝트 내부 소스와 로컬 workspace는 분석하고, 설치되지 않은 외부 crate의
내부 구현은 로드하지 않는다. 외부 crate는 architecture 단계에서 library
boundary로 남는다. 설정은 LSP 초기화 시 provider에 전달하며, 사용자 프로젝트나
Cargo 설정은 수정하지 않는다.

### 검증

- 실제 meeting-overlay Rust fixture에서 설정 전 `SendError` panic 재현
- 동일 fixture 설정 후 panic 없음, strict exit 0
- Rust 문서 1개, 관계 16개, Tauri RPC 4개 유지
- 12개 언어 semantic gate: 12/12
- framework semantic gate: 84/84
- meeting-overlay 외부 E2E: routes 53, React components 28, Tauri JS 4,
  Tauri Rust 4, source comparison 통과

## 61. 대형 Rust workspace 메모리 ceiling

Nushell 실제 프로젝트(약 1,465개 Rust 파일)를 최신 provider로 재검증했다.
외부 crate 로딩 panic은 없어졌지만 rust-analyzer가 workspace 내부 44개 crate를
정밀하게 올리는 동안 약 5.8~6.0GB를 사용했다. 약 402초 동안 완료되지 않은
실행은 메모리 압박 때문에 중단했다.

외부 crate·build script·proc-macro·test target을 줄이는 설정과 임시
`rust-project.json` 실험도 했지만 workspace 내부 crate 자체가 메모리의 주원인이라
유의미한 감소가 없었다. 임시 생성 로직은 제거했다. 따라서 현재 Rust 대형
workspace는 **정확도 우선 provider 한계가 확인된 상태**이며, 성공으로 포장하지
않는다. package 단위 provider 세션과 cross-crate dependency closure를 설계한
후속 작업이 필요하다.

## 62. architecture 단계의 소스 문자열 중복 보관

### 문제

`SourceSnapshot`은 framework 분석과 cache key 계산을 위해 프로젝트 소스
문자열을 보관한다. architecture 단계에서 이 문자열을 새
`HashMap<String, String>`으로 복사하면, provider 결과와 architecture 후처리가
겹치는 순간 대형 프로젝트의 소스 메모리가 일시적으로 두 배가 될 수 있었다.

### 해결

framework와 cache key 단계가 끝난 뒤 architecture builder가 snapshot의 source
문자열을 소유권 이동으로 소비하도록 변경했다. 이제 같은 문자열을 다시 복사하지
않고 architecture 전용 경로 인덱스만 추가로 만든다. architecture cache hit이면
snapshot을 소비하지 않으므로 기존 cache 동작도 유지된다.

### 검증

- `cargo fmt --manifest-path rust/Cargo.toml -- --check`: 통과
- Rust release tests: 76/76 통과
- release build: 성공
- 12개 언어 semantic gate: 12/12 통과
- framework semantic gate: 84/84 통과
- `meeting-overlay-assistant` 외부 E2E: 통과

이 변경은 provider의 의미 분석 결과나 관계를 바꾸지 않는 메모리 최적화다.

## 63. cache hit에서도 전체 소스 본문을 먼저 읽던 문제

### 문제

기존 index 시작 단계는 provider, framework, architecture 캐시가 모두 유효한
경우에도 모든 소스 파일의 본문을 `SourceSnapshot`에 올렸다. 캐시 hit에서는
본문이 실제로 필요하지 않은데도 대형 프로젝트의 메모리와 초기 처리 시간이
증가했다.

### 해결

- 첫 단계에서는 파일 checksum과 경로만 수집한다.
- framework cache miss일 때만 framework 분석 직전에 소스 본문을 읽는다.
- architecture cache miss일 때만 architecture 직전에 소스 본문을 읽는다.
- 1MB 초과 파일은 기존 정책대로 checksum과 빈 source boundary만 유지한다.

본문을 읽는 시점만 늦추며 checksum과 cache key는 기존과 같은 입력을 사용한다.
사용자 프로젝트 파일이나 설정은 변경하지 않는다.

### 검증

- Rust release tests: 77/77 통과
- release build 및 format check: 성공
- 12개 언어 semantic gate: 12/12 통과
- fresh `meeting-overlay-assistant` E2E: routes 53, components 28,
  JS invokes 4, Rust commands 4, source comparison 통과

## 64. 변경 파일의 영향 범위가 cache hit에 반영되지 않던 문제

### 문제

기존 모듈 cache key는 해당 모듈의 파일이 바뀌었는지는 감지했지만, 다른
모듈이 그 파일을 import하고 있을 때 importer 모듈까지 provider 재분석 대상에
포함하지 않았다. 그러면 importer의 관계 결과가 이전 cache에 남을 수 있었다.

### 해결

- 프로젝트별 source checksum manifest를 AppData cache에 저장한다.
- 이전 language index와 architecture output의 `IMPORTS` 관계를 역방향으로
  읽어 변경 파일의 importer를 재귀적으로 계산한다.
- 영향 범위에 포함된 모듈은 language cache를 사용하지 않고 provider를 다시
  실행한다.
- 이전 manifest가 없거나 영향 정보를 확인할 수 없는 첫 실행은 안전하게 전체
  language cache를 무효화한다.
- 관계를 새로 추정해 그래프에 추가하지 않고, 기존 provider/architecture가
  기록한 import 관계만 cache 영향 계산에 사용한다.

### 검증

- Rust release tests: 78/78 통과
- 첫 semantic gate 실행: 12/12 통과 및 source manifest 생성
- 동일 입력 재실행: 12개 언어 모두 cache hit, 12/12 통과
- 변경 dependency와 importer를 함께 무효화하는 단위 테스트 통과

이 기능은 provider의 cross-file 의미를 보존하면서 변경 범위 밖 모듈의 재분석을
피한다. provider 내부의 파일별 incremental protocol 자체를 대체하지는 않는다.

## 65. framework 심볼 후보의 불필요한 반복 복사

framework route·fact가 handler 심볼을 찾을 때 후보가 하나뿐이어도 매번 후보
벡터를 복사하고 정렬했다. 후보가 많은 모호한 경우에만 필요한 작업이 일반적인
단일 후보 경로에서도 수행되는 구조였다.

단일 후보는 즉시 반환하고, 2개 이상일 때만 기존 dedupe·implementation score
정렬을 수행하도록 변경했다. 심볼 선택 규칙과 모호한 경우의 `None` 처리는
그대로 유지된다. 일반 경로의 후보 복사 비용은 O(k)에서 O(1)로 줄고, 모호한
후보의 기존 비용은 유지된다.

검증:

- Rust release tests: 78/78 통과
- release build 및 format check: 성공
- 12개 언어 semantic gate: 12/12 통과
- framework semantic gate: 84/84 통과

## 66. Rust LSP가 Cargo reload 뒤 함수 심볼을 비우던 문제

### 문제

`meeting-overlay-assistant`의 Rust 파일은 실제 소스에 Tauri command 함수가
4개 있었지만, fresh provider 실행의 마지막 `documentSymbol` 응답에는 구조체만
남고 함수가 사라지는 경우가 있었다. 그 결과 `RPC_ENDPOINT` fact는 발견해도
handler 심볼을 확정하지 못해 `HANDLES` 관계가 0개가 됐다.

### 원인

rust-analyzer는 Cargo workspace를 다시 읽는 동안 시점별로 서로 다른
`documentSymbol` 결과를 반환한다. 기존 코드는 시작 후 5초 기다린 뒤 마지막
응답만 사용했기 때문에, 앞서 받은 유효한 함수 심볼을 뒤의 빈/부분 응답으로
덮어썼다. 테스트 staging에서도 실제 프로젝트의 `Cargo.lock`을 함께 복사하지
않아 이 reload 변동이 더 커졌다.

### 해결

- Rust는 초기 LSP 응답부터 polling한다.
- 여러 응답 중 callable 심볼 수가 가장 많고, 동률이면 전체 심볼 수가 가장
  많은 응답을 유지한다.
- 이후 응답이 비거나 줄어들어도 이미 수집한 실제 심볼을 덮어쓰지 않는다.
- 외부 Rust E2E staging은 원본에 `Cargo.lock`이 있을 때 함께 복사한다.
- 소스에서 함수 이름을 추정해 관계를 만드는 fallback은 추가하지 않았다.

### 검증

- `cargo fmt --manifest-path rust/Cargo.toml -- --check`: 통과
- release build: 성공
- fresh `meeting-overlay-assistant` E2E: exit 0
- Rust documents 1개, relations 16개, Tauri RPC 4개, `HANDLES` 4개
- source comparison: command 4개와 index 4개 일치
- 전체 외부 게이트: FastAPI 53/53, React 28, Tauri JS 4, Tauri Rust 4 통과

## 67. Go 대형 module에서 gopls 세션 timeout과 전역 coverage 누락

### 문제

실제 `D:\visual_map_reliability_lab\go-etcd`를 strict 모드로 실행했을 때
처음에는 842개 파일 중 414개만 indexed되고, `server` module의 389개 파일이
`native LSP session timeout`으로 누락됐다. 400파일 안팎의 Go module이 일반
대규모 기준 500파일 아래라서 모든 심볼에 대해 호출·타입 질의를 수행한 것이
원인이었다. 또한 한 파일의 document-symbol timeout이 발생하면 이후 파일까지
전역적으로 질의를 생략하는 상태였다.

### 해결

- gopls workspace는 250파일 초과부터 대규모 경로로 전환한다.
- exported package-level function과 provider가 반환한 boundary에 집중하고,
  private receiver method 전체를 무차별 질의하지 않는다.
- Go도 provider-backed map enrichment 대상에 포함해 실제 호출 관계는 유지한다.
- document-symbol degradation 상태를 전역이 아니라 파일별로 관리한다. 한
  파일의 provider 문제로 다른 파일을 누락시키지 않는다.
- Go build constraint 파일은 기존처럼 제외하고, 주석과 `package` 선언만 있는
  package marker 파일은 `go-package-marker`로 명시적으로 제외한다. 가짜
  심볼이나 호출 관계는 만들지 않는다.

### 검증

- Go 단일 `server` module: 392개 중 388개 indexed, build constraint 4개 제외,
  누락 0, 오류 0, provider 약 18.6초
- 전체 `go-etcd`: 842개 중 742개 indexed, build constraint 39개와 package
  marker 61개 제외, 누락 0, strict exit 0
- 관계: 5,802개(호출 3,252개, 참조 2,550개)
- architecture: module 124개, flow 489개, 추정 관계 0개
- 동일 입력 cache hit: provider merge 약 76ms, 모든 Go module cache hit
- Rust release tests: 80/80 통과

## 68. Go 문자열을 import로 오인해 외부 library edge를 만들던 문제

### 문제

Go import parser가 모든 줄에서 첫 번째 따옴표 문자열을 읽고 있었다. 실제
`client/pkg/testutil/leak.go`의 문자열
`"created by testing.RunTests"`가 import로 오인되어 외부 library node와
중복 `USES_LIBRARY` edge가 생성됐다.

### 해결

Go는 `import "pkg"` 또는 `import (...)` 블록 내부의 import spec만 읽도록
제한했다. 일반 코드의 문자열은 import로 처리하지 않는다.

### 검증

- `go_import_parser_ignores_quoted_strings` 회귀 테스트 통과
- Rust release tests: 80/80 통과
- 추정 관계를 생성하는 fallback은 추가하지 않음

## 69. Framework gate가 번들 provider가 아닌 PATH 도구를 사용하던 문제

### 문제

Framework gate는 `-ProvidersRoot` 인자를 받아도 TypeScript 외의 fixture
실행에는 provider 경로를 넘기지 않았다. 따라서 테스트 컴퓨터의 PATH에 있는
JDK/LSP가 선택될 수 있었고, 같은 코드가 실행 환경에 따라 Rust/Axum 같은
교차 파일 handler 연결을 통과하거나 실패했다.

### 해결

`run-framework-index-gate.ps1`가 `-ProvidersRoot`를 모든 언어 실행에 공통으로
전달하도록 수정했다. C/C++만 별도 provider 경로가 지정되면 그 경로를 우선하고,
지정하지 않으면 공통 번들 경로를 사용한다. 엔진에 추정 handler fallback은
추가하지 않았다.

### 검증

- bundled provider로 TypeScript/Express 통과
- bundled provider로 Rust/Axum 교차 파일 handler 통과
- bundled provider로 Python/Flask 통과

## 70. 혼합 프로젝트의 명시적 제외 파일이 strict 전체 실패로 처리되던 문제

### 문제

Flutter/Dart 대형 프로젝트에서 Dart는 696개 중 692개를 분석하고 누락 0으로
끝났지만, 프로젝트에 함께 있는 C 파일 하나가 compile context 부재로
`excluded`가 되면서 전체 strict gate가 실패했다. 명시적인 정책 제외를 provider
누락과 같은 실패로 취급한 것이다.

### 해결

strict gate는 이제 `files_missing > 0`인 경우와 provider 실행 실패·잘못된 출력·빈
의미 결과만 실패로 처리한다. `excluded`는 coverage에 이유를 남긴 정상적인
정책 결과로 허용한다. 따라서 C 파일을 억지로 분석하거나 관계를 추정하지 않는다.

### 검증

- Rust release tests: 82/82 통과
- Dart/Flutter DevTools: Dart 696개 중 692개 indexed, 4개 명시적 제외,
  누락 0, strict exit 0
- JavaScript 1개 indexed, 전체 결과 exit 0

## 71. Rust 대형 workspace의 `workspace/symbol` 일괄 수집 실험

### 결과

Nushell 1,465개 Rust 파일에서 파일별 `documentSymbol` 요청을
`workspace/symbol` 1회 우선 방식으로 바꿔 측정했지만, provider가 workspace 전체를
색인하는 비용이 더 커졌다. provider 단계가 기존 약 397초에서 약 449초로
늘었고, 관계도 76개로 줄었다.

### 판단

이 방식은 VisualMap의 정확도와 속도 모두 개선하지 못하므로 제거했다. Rust 대형
workspace는 현재 provider 자체의 Cargo 분석 비용이 지배적이며, 그 결과를
보완하려고 이름 기반 관계나 하드코딩 fallback을 추가하지 않는다. 현재 코드는
검증된 기존 경로를 유지한다.

## 72. Python 실제 프로젝트 검증 결과

`meeting-overlay-assistant/legacy/backend`를 provider-only로 실행해 Python
coverage와 FastAPI route 연결을 확인했다.

- Python 51/51 indexed
- 누락 0, 명시적 제외 0
- FastAPI route 53개, handler 연결 53개
- CALLS 관계 37개
- 외부 패키지·타입 관련 진단은 warning으로 기록되고 분석 실패로 승격하지 않음
- source comparison 통과

## 73. Rust 대형 workspace `didOpen` 제한 실험

Nushell 1,465개 파일에서 rust-analyzer에 보내는 `didOpen`을 256개로 제한하는
실험을 했다. provider 메모리가 약 6.3GB까지 올라간 채 7분 이상 결과를 반환하지
않았고, 기존 경로보다 개선됐다고 판단할 근거가 없었다.

따라서 이 변경은 제거했다. Rust 대형 프로젝트는 provider가 workspace 전체
Cargo graph를 준비하는 비용이 지배적이며, editor buffer를 임의로 줄여서 파일
누락을 만들지 않는다.

## 74. 작은 Rust 프로젝트의 부분 document symbols로 Tauri 연결이 사라지던 문제

### 문제

최신 외부 프로젝트 게이트에서 `meeting-overlay-assistant`의 Rust 파일은
`indexed`였지만, 첫 번째 rust-analyzer `textDocument/documentSymbol` 응답이
구조체와 모듈만 반환했다. 함수 심볼이 빠진 상태에서 다음 단계가 진행되어
Tauri `HANDLES` 관계가 0개가 됐다. 파일 누락이나 이름 추정 문제가 아니라
provider 초기화 응답을 너무 일찍 확정한 문제였다.

### 해결

작은 Rust 프로젝트(semantic file 8개 이하)에서 document-symbol 응답에
callable 심볼이 없으면 최대 20회까지 기존 요청을 재시도하고, 매번 callable
수와 전체 심볼 수가 더 많은 응답을 유지한다. 대형 workspace의 재시도 횟수는
그대로 둬서 provider 비용을 불필요하게 키우지 않는다. 관계를 이름으로 만들거나
Tauri 명령을 하드코딩하는 fallback은 추가하지 않았다.

### 검증

- 최신 bundled rust-analyzer 재실행: 19.9초, 문서 1개, `CALLS` 16개
- Tauri `HANDLES` 4개: `register_ui_rects`, `start_live_audio_stream`,
  `prewarm_live_audio_stream`, `stop_live_audio_stream`
- strict exit 0
- language cache schema `v115 → v116`으로 이전 빈 결과를 재사용하지 않음

## 75. 최신 언어·외부 프로젝트 회귀 검증 결과

Rust provider 재시도 수정 이후 최신 release 바이너리로 다시 검증했다.

- 12개 언어 semantic gate: `12/12`, skipped `0`
- `meeting-overlay-assistant` 외부 gate: 통과
  - Python route `53/53`
  - React component `28/28`
  - Tauri JavaScript invoke `4개`
  - Tauri Rust command `4개`, `HANDLES 4개`
  - source comparison 통과
- Rust Tauri provider: 문서 `1`, `CALLS 16`, provider 약 `19.9초`
- `nopCommerce` C# 최신 재실행: `3614`개 중 `3613`개 indexed,
  별도 `ClearPluginAssemblies.sln` 소속 `Program.cs` 1개는
  `project-config`로 명시적 제외, missing `0`

이 결과는 12개 언어가 대표 fixture와 실제 혼합 프로젝트에서 동작한다는
증거다. 다만 이것만으로 모든 대형 프로젝트에서 완전한 호출관계를 보장하는
것은 아니다. 특히 Nushell급 Rust workspace는 provider 자체가 약 7분 이상
걸리고 공개 경계 밖의 호출계층 질의는 생략한다. 따라서 현재 최종 미완료 항목은
Rust 대형 workspace의 provider 비용과 각 프레임워크 pack의
전수 실프로젝트 인증이다.

## 76. Rust 대형 workspace의 공개 경계 호출관계 복구

### 변경

Rust 1,000개 초과 workspace에서도 무차별 호출계층 질의는 하지 않고,
기존 `large_symbol_is_map_boundary`가 공개 API로 판정한 심볼에 한해
provider의 outgoing-call 질의를 수행하도록 변경했다. 이름 검색이나 호출
fallback은 사용하지 않는다.

### Nushell 검증

- Rust 파일 `1465/1465`, missing `0`
- `CALLS 1474`, `REFERENCES 1775`
- provider merge 약 `457.7초`, architecture 약 `0.36초`
- 이전 Rust 대형 호출 생략 결과의 `CALLS 34`보다 VisualMap 흐름 정보 증가
- strict exit `0`

정확한 공개 경계 흐름을 얻는 대신 provider 시간이 약 `397초 → 458초`로
늘었다. 따라서 현재 Rust 대형 분석의 병목은 bridge가 아니라
rust-analyzer의 Cargo workspace 초기화와 provider 호출 질의다. 3~4분 목표는
아직 달성하지 못했으며, 이를 줄이려면 rust-analyzer 자체의 incremental/index
기능을 별도 검증해야 한다. 관계를 줄여 시간을 숨기는 방식은 채택하지 않는다.

## 77. 현재 bridge 시간복잡도·메모리 감사

현재 bridge의 프로젝트 내부 단계는 다음 비용 구조다.

- 파일 탐색·메타데이터 hash: 파일 수를 `F`라 할 때 `O(F)`
- 변경 영향 범위: manifest와 import 역색인을 이용해
  `O(F + 변경 파일 + 영향 모듈)` 평균 비용
- provider 결과 병합·중복 제거: 문서 `D`, 관계 `R`에 대해 HashSet 기반
  평균 `O(D + R)`
- architecture 생성: 노드·edge를 결정적 BTreeMap에 넣으므로
  평균 `O((N + E) log N)`; 중복 evidence는 HashSet으로 선형 스캔을 피함
- framework 분석: pack 신호를 캐시하고 후보 파일로 줄여
  `O(pack 수 × 신호 수 × 후보 파일)`에 가깝게 동작

실측상 bridge 후처리는 대형 프로젝트에서도 수 초 수준이고 provider가 지배적이다.

| 프로젝트 | provider | framework | architecture | 결과 |
|---|---:|---:|---:|---|
| Vendure | 약 48초 | 약 4.8초 | 약 1초 | TS 2,514개 중 17개 정책 제외, missing 0 |
| nopCommerce | 약 79초 | 약 6초 | 약 2.2초 | C# 3,614개 중 1개 project-config 제외, missing 0 |
| Nushell | 약 458초 | 약 0.9초 | 약 0.4초 | Rust 1,465/1,465, CALLS 1,474 |

메모리도 bridge보다 provider가 지배한다. Nushell rust-analyzer가 약 6.2GB를
사용했다. 따라서 현재 추가로 bridge에 복잡한 분산 처리나 새 자료구조를 넣는
것은 실측 근거가 부족하다. 다음 성능 개선은 rust-analyzer/gopls 등 provider의
workspace 초기화·증분 index를 별도 다루는 작업이어야 한다.

## 78. 84개 framework provider-backed 전수 게이트

framework catalog의 84개 pack을 각각 임시 프로젝트로 만들고, bundled
provider를 실제 실행한 뒤 다음을 검사했다.

- provider language status가 `indexed`인지
- framework pack이 실제로 감지되는지
- 선언된 모든 fact가 생성되는지
- fact의 source file/source range가 실제 파일과 일치하는지
- unresolved handler가 무조건 `HANDLES`로 승격되지 않는지
- resolved route/RPC가 `HANDLES` 관계를 갖는지

검증 결과:

- provider-backed gate: `84/84`
- catalog gate: `84/84`
- semantic self-test: `84/84`
- 실패 0, 추정 handler fallback 0

이제 framework pack 자체는 provider-backed fixture 기준으로 완료 판정할 수
있다. 다만 실제 생태계의 모든 버전별 DSL, 매크로, 자동 등록, 런타임 동적
구성까지 지원한다는 뜻은 아니다. 그런 경우에도 현재 계약대로 확인된 fact만
저장하고 확인되지 않은 연결은 만들지 않는다.

## 79. rust-analyzer SCIP CLI 대체 경로 실험

bundled `rust-analyzer scip` 명령을 소규모 Tauri Rust overlay에 직접 실행해
LSP보다 빠른 경로인지 확인했다. 기본 SCIP 명령은 proc-macro와 build-script
의존성을 대량으로 로드했고, 약 2GB 메모리를 사용한 채 결과를 반환하지 않았다.

따라서 현재 VisualMap bridge의 LSP 경로를 SCIP CLI로 교체하지 않았다. 이
실험의 부분 SCIP 결과는 최종 분석 결과로 사용하지 않는다.

## 80. Go 대형 프로젝트 최신 release 재검증

현재 cache schema와 bundled `gopls`로 `go-etcd`를 다시 실행했다.

- `842`개 발견, `742`개 indexed
- `100`개는 build constraint `39`개와 package marker `61`개로 명시적 제외
- missing `0`, strict exit `0`
- `CALLS 3252`, `REFERENCES 2550`
- provider 약 `95.5초`, framework 약 `0.03초`, architecture 약 `0.36초`

Go provider 경고는 진단으로 보존하며 관계를 추정하는 fallback으로 바꾸지
않았다. 대형 Go도 현재 VisualMap의 파일 범위·호출·외부 경계 계약을 통과한다.

## 81. 전역 개발 도구 없이 bundled provider 검증

사용자 PATH에 의존하지 않도록 테스트 프로세스의 PATH를
`C:\Windows\System32`로 제한하고 `--providers-root .\providers`만 전달했다.
또한 별도의 `LOCALAPPDATA`를 사용해 기존 언어 캐시가 없는 상태에서
검증했다.

- 12개 언어 semantic gate: `12/12`
- provider doctor: TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust,
  PHP, Ruby, Dart 모두 `READY`
- 전역 JDK/Go/Python/Ruby/Dart 경로를 사용하지 않음
- C/C++ completion gate도 캐시 miss에서 실제 clangd 실행 후 `5개 문서`,
  `17개 관계`, `C/C++ 각각 indexed`, 중복 문서·관계 0개로 통과

따라서 현재 번들 provider 실행 구조는 사용자 개발환경의 PATH를 수정하지
않고도 기본 언어 분석을 시작할 수 있다. C/C++는 한 번의 clangd 컴파일
컨텍스트 분석을 C와 C++ 파일 범위로 나눠 저장하므로 provider 중복 실행도
발생하지 않는다.

## 82. 언어 캐시의 중복 직렬화

### 문제

provider 결과를 정규화하는 공통 함수가 캐시를 한 번 저장한 뒤, 호출자가
provider 진단을 추가하고 같은 문서·관계를 다시 저장하고 있었다. 결과는
같았지만 대형 프로젝트에서 언어별 캐시 JSON 직렬화와 디스크 쓰기가
불필요하게 두 번 발생했다.

### 해결

공통 정규화 함수는 분석 결과만 반환하도록 바꾸고, 최종 provider 진단이
합쳐진 호출자 단계에서 언어 캐시를 한 번만 기록하게 했다. 캐시 내용에는
진단이 포함된 최종 결과만 남는다.

### 검증

- Rust release tests: `82/82`
- 12개 언어 semantic gate: `12/12`
- C/C++ cache-miss clangd gate: 문서·관계 중복 `0`

## 83. 단일 언어 provider의 임시 SCIP 잔류

### 문제

여러 언어가 공유하는 provider job은 SCIP 파일을 읽은 뒤 정리했지만, 단일
언어 provider 경로는 읽기 실패 여부와 관계없이 임시 `.scip` 파일을 남길 수
있었다. 반복 분석에서 작업 디렉터리의 임시 파일이 누적될 여지가 있었다.

### 해결

단일 언어 경로도 SCIP 읽기 결과를 메모리에 받은 직후 성공·실패 양쪽에서
임시 파일을 삭제하도록 통일했다. 사용자 프로젝트의 소스·설정 파일은
삭제하지 않는다.

### 검증

- Rust release tests: `82/82`
- 12개 언어 semantic gate: `12/12`
- provider cache 결과와 최종 진단 출력은 기존 계약과 동일
- cache miss Python provider 실행 후 잔류 `.scip` 파일 `0개`

## 84. Rust 소규모 Cargo 프로젝트의 부분 symbol 응답 재사용

### 문제

Tauri처럼 proc-macro와 dependency 로딩이 있는 작은 Rust 프로젝트에서
rust-analyzer가 첫 `documentSymbol` 응답으로 구조체만 반환할 수 있었다.
기존 재시도 횟수는 있었지만 첫 응답이 LSP 요청 캐시에 들어가서 같은 부분
응답을 반복 조회했고, 함수·호출 관계가 `0`개가 되는 경우가 있었다.

### 해결

파일별 `documentSymbol`은 현재 분석에서 한 번만 필요하므로 요청 캐시를
우회해 재시도하도록 했다. Rust 소규모 workspace는 provider가 Cargo 상태를
완성할 시간을 갖도록 bounded retry를 적용하고, 대형 workspace의 요청
범위 제한 정책은 유지한다.

### 검증

- Tauri Rust cache miss: 문서 `1`, 관계 `16`, RPC `4`, HANDLES `4`
- 전체 external E2E: Python routes `53/53`, React components `28`, Tauri JS
  invoke `4`, Tauri Rust RPC/HANDLES `4/4`
- 별도 `LOCALAPPDATA` cache miss 12개 언어 semantic gate: `12/12`
- source comparison: 통과

## 85. external E2E staging 경로와 제한 PATH 문제

### 문제

external gate가 기본 staging을 저장소의 `build` 아래에 만들면 bridge의
정상적인 build 디렉터리 제외 정책과 충돌해 architecture module/flow가
비어 보일 수 있었다. 또한 PATH를 시스템 디렉터리로 제한하면 비교 스크립트가
`powershell` 명령을 찾지 못했다.

### 해결

external gate의 기본 staging을 OS 임시 디렉터리로 옮기고, 비교 단계는
현재 PowerShell 7의 `$PSHOME` 실행 파일을 우선 사용하도록 수정했다. 사용자
프로젝트와 provider PATH 정책은 변경하지 않는다.

### 검증

- 제한 PATH + bundled providers로 external gate 전체 통과
- source comparison 통과
- 기본 output: `%TEMP%\visual-map-external-gate`

## 86. rust-analyzer cache priming 비활성화 실험

VisualMap 질의가 파일별 심볼을 직접 요청하므로 `cachePriming`을 끄는
최적화를 실험했다. Tauri cache miss에서 관계 `16`, HANDLES `4`는 유지됐지만
provider 실행 시간은 기존 약 `16~17초`보다 개선되지 않고 약 `21.6초`로
늘었다. 따라서 정확도·성능상 이득이 확인되지 않은 설정은 제품 기본값에
남기지 않고 원복했다.

## 87. 대형 Rust LSP 원본 응답 캐시 제거 실험

대형 workspace에서 `LspConnection`의 원본 JSON 응답 캐시를 끄면 메모리가
줄어드는지 측정했다. 소규모 Tauri에서는 관계 `16`, HANDLES `4`가 유지됐고,
Nushell `1,465`개 Rust 파일에서는 `CALLS 1,475`, `REFERENCES 1,777`,
provider 약 `450.3초`가 나왔다. 그러나 peak 메모리는 약 `6.34GB`로 기존
약 `6.2GB`보다 줄지 않았다. 속도 차이도 측정 오차 범위이므로 이 변경은
원복했다. 대형 Rust 병목은 bridge의 응답 캐시가 아니라 rust-analyzer의
workspace 분석 비용으로 판단한다.

## 88. 대형 Rust workspace의 불필요한 didOpen 전송

### 문제

Rust 대형 workspace에서 bridge가 모든 semantic 파일의 전체 소스 내용을
`textDocument/didOpen`으로 다시 전송하고 있었다. rust-analyzer는 Cargo
workspace의 파일을 디스크에서 읽을 수 있으므로, 이 전송은 provider의
workspace 메모리와 초기 입력 비용을 늘릴 가능성이 있었다.

### 해결

대형 Rust workspace에서만 처음 `256`개 문서를 editor buffer로 열고, 나머지
파일은 기존처럼 document symbol/call query 대상으로 유지한다. 소규모
프로젝트와 다른 언어의 동작은 바꾸지 않는다.

### 검증

- Nushell: 문서 `1,467`개, 관계 `3,252`개로 기존과 동일
- Rust provider: `457.7초 → 404.3초`(약 `11.7%` 감소)
- Tauri external E2E: Rust RPC `4/4`, HANDLES `4/4`, source comparison 통과
- 대형 workspace 진단은 provider 자체 경고가 늘어날 수 있으므로 진단 수를
  줄이기 위해 오류를 숨기지 않는다.

## 89. Rust 2018 로컬 use 경로의 외부 라이브러리 오분류

### 문제

Rust architecture resolver가 use crate::types::...는 내부 파일로
해석했지만, Rust 2018에서 흔한 use types::...는 crate 접두사가
없다는 이유로 외부 라이브러리로 기록했다. 실제 src/types.rs가 있어도
VisualMap에 USES_LIBRARY가 생기고 내부 IMPORTS가 빠졌다.

### 해결

use 경로의 첫 모듈에 대응하는 프로젝트 파일(module.rs 또는
module/mod.rs)이 실제로 있을 때만 내부 import로 연결한다. 파일이
없거나 여러 후보가 확정되지 않으면 기존처럼 외부 경계로 남긴다.

### 검증

- Rust architecture 단위 테스트 83/83
- Rust fixture: file:src/main.rs → file:src/types.rs
  IMPORTS / resolution=internal
- 존재하지 않는 외부 이름은 계속 외부 라이브러리로 처리

## 90. Rust mod 선언 누락

Rust 파일이 use 없이 mod service; 형태로 하위 모듈을 선언하면 기존
architecture import parser가 파일 관계를 만들지 못했다. 실제 module.rs
또는 module/mod.rs가 존재할 때만 mod 선언을 내부 IMPORTS로 연결하도록
보완했다. Rust architecture 테스트는 84/84로 통과했다.

## 91. VisualMap 범위 최종 회귀 판정

최신 Rust import resolver와 대형 workspace 최적화를 포함한 binary로 최종
검증했다.

- 12개 언어 semantic gate: 12/12
- coverage 품질 audit: 12/12
- architecture 품질 audit: 12/12
- 중복 문서·추정 관계: 0
- framework provider gate: 84/84
- Rust framework gate: 8/8
- meeting-overlay-assistant external E2E: Python route 53/53, React
  component 28, Tauri JS invoke 4, Tauri Rust RPC/HANDLES 4/4,
  source comparison 통과
- Rust unit test: 84/84

이 판정은 VisualMap의 정적 구조·호출·import·framework map 범위다. 런타임
동적 dispatch, framework auto-import, ORM/DB 의미 분석, provider가 지원하지
않는 compiler-level type hierarchy는 별도 범위로 남긴다.

## 92. 모듈 병합 시 전역 LSP 실패가 단위 missing으로 남는 문제

### 문제

여러 언어 모듈을 하나의 provider 작업으로 병합할 때, 경로가 없는 LSP
타임아웃 진단이 개별 모듈에 전달되지 않았다. 그 결과 실제로는 구조 지도만
남겨야 하는 모듈이 `provider-failed`와 `missing`으로 표시됐다.

### 해결

병합부도 provider 진단을 검사해 의미 문서가 하나도 없고 LSP가 종료된 경우
해당 모듈을 `excluded`로 기록한다. 최종 coverage도 언어 집계의 명시적 제외
수를 단위 coverage에 반영한다. 추정 관계나 하드코딩된 내부 호출은 추가하지
않는다.

### 검증

- cpp-opencv Python: `299 = indexed 6 + excluded 293`, missing `0`
- cpp-opencv C/C++: compile context 부재를 excluded로 기록, missing `0`
- Rust unit test `85/85`

## 93. 구형 .NET SDK와 C# 의미 분석 결과의 오분류

### 문제

Windows Phone/Silverlight처럼 현재 내장 .NET SDK에 없는 구형 SDK를 요구하는
프로젝트는 provider가 빈 SCIP를 반환했다. 이를 일반 provider missing으로
표시하면 VisualMap 구조 지도 자체가 실패한 것처럼 보였다.

### 해결

프로젝트 파일이 실제로 WindowsPhone 또는 Silverlight 대상이고, 해당 단위의
모든 C# 파일이 그 프로젝트에 속할 때만 의미 분석을 `excluded`로 처리한다.
현대 .NET 프로젝트에는 이 정책을 적용하지 않는다.

### 검증

- cpp-opencv 구형 C#: `15/15 excluded`, missing `0`
- nopCommerce 현대 C#: `3613/3614 indexed`, missing `0`

## 94. 대형 Rust workspace의 build.rs 반복 스캔

### 문제

Cargo workspace의 각 crate마다 프로젝트 전체의 `build.rs`를 다시 순회해
외부 도구 필요 여부를 확인했다. Nushell에서 이 반복이 분석 시간을 크게
늘렸다.

### 해결

workspace root를 찾고, 프로세스 안에서 workspace별 검사 결과를 한 번만
캐시한다. `target`, `.git`, `node_modules`는 계속 제외한다.

### 검증

- Nushell: 약 `158.5초 → 12.3초`
- Rust `1465`개 파일: `excluded`, missing `0`
- provider 오류와 구조 출력은 유지

## 95. Java/Spring provider와 framework pack의 소유권·HANDLES 신뢰성

### 문제

Spring 프로젝트에서 언어 provider가 반환한 실제 메서드 심볼과 framework
pack이 문자열로 추출한 handler 이름이 서로 다른 식별자를 사용할 수 있었다.
또한 Spring Boot, Spring MVC, Spring WebFlux처럼 같은 애노테이션 문법을
공유하는 pack이 동시에 route를 소유하면 API가 중복되고, `@GetMapping`의
bare/relative 형태나 클래스 prefix가 있는 route는 경로 또는 HANDLES가 빠질
수 있었다. JAX-RS와 Quarkus도 같은 계열의 소유권 충돌 가능성이 있었다.

### 해결

- provider reference의 파일·범위 근거를 우선 사용해 framework handler를
  실제 프로젝트 정의 심볼로 정규화했다.
- 동일 route 후보는 모듈에서 확인된 web stack과 framework 소유권으로 한
  pack만 선택하도록 했다.
- Spring의 클래스 prefix, 메서드 mapping, bare/relative mapping을 한 경로로
  합성했다.
- JAX-RS/Quarkus도 선언한 framework identity와 route owner가 맞을 때만
  결과를 내도록 제한했다.
- 근거가 여러 개라 확정할 수 없는 경우에는 전역 이름 추정으로 연결하지
  않고 unresolved 상태를 유지했다.

### 검증

- 고정된 Spring 실저장소 필드 검증에서 route/HANDLES `18/18`
- Spring MVC/WebFlux 소유권, 클래스 prefix, bare/relative mapping 회귀 테스트
- 전체 framework pack fixture gate 통과

## 96. C#/FastEndpoints와 Python/FastAPI 제품 어댑터 경계

### 문제

C#에서는 ASP.NET 계열 pack들이 공통 애트리뷰트 문법을 공유해 잘못된 pack이
활성화될 수 있었고, FastEndpoints의 `Configure()` 정적 설정은 일반 route
추출만으로 endpoint와 `ExecuteAsync`를 연결하기 어려웠다. FastAPI에서는
router prefix와 include 경로가 분리된 경우 최종 API 경로와 handler 내부의
정적 import 호출이 제품 어댑터까지 보존되지 않을 수 있었다.

### 해결

- ASP.NET Core/MVC/Web API/Minimal API pack의 activation 및 route owner를
  실제 프로젝트 identity에 맞게 좁혔다.
- FastEndpoints는 실제 `Configure()`의 verb/path와 같은 클래스의
  `ExecuteAsync`를 source-backed route/HANDLES로 투영했다.
- FastAPI는 router/include prefix를 합성하고 provider reference 또는 확인된
  정적 import만 CALLS로 유지했다.
- 테스트 namespace로 향하는 잘못된 짧은 이름 연결은 제품 어댑터에서
  확정 관계로 승격하지 않았다.

### 검증

- CleanArchitecture: 제품 adapter `12 routes / 12 HANDLES`
- FastAPI+TypeScript monorepo: 엔진 `39 routes / 39 HANDLES`
- FastAPI 로그인 handler에서 `authenticate`, `create_access_token`의
  source-backed 호출 확인

## 97. 다중 앱 인스턴스와 엄격 Clippy 경계

### 문제

workspace lock은 한 앱 프로세스 안의 동시 작업만 직렬화했다. 사용자가 앱을
두 번 실행하면 각 프로세스는 서로 다른 메모리 lock을 가지므로 같은
workspace를 동시에 갱신할 수 있었다. 별도로 Rust의 엄격 Clippy에는 기존
lint가 누적되어 새 변경의 경고를 신뢰하기 어려운 상태였다.

### 해결

- Tauri의 OS 단일 인스턴스 plugin을 사용해 두 번째 프로세스의 시작을 막고,
  실행 인자는 기존 앱으로 전달하도록 했다.
- 언어 엔진과 Tauri backend의 기존 Clippy 경고를 제거하고
  `--all-targets -- -D warnings`를 회귀 gate로 사용했다.

### 검증

- 실제 두 번 실행에서 하나의 앱 프로세스만 workspace를 소유
- 언어 엔진 및 Tauri backend 엄격 Clippy 통과
- 현재 전체 회귀: 엔진 `135/135`, Tauri `248/248` + ignored field `5`,
  frontend `149/149`

남은 경계는 의도적으로 앱을 여러 프로세스로 동시에 운영해야 하는 제품
요구가 생기는 경우다. 그때는 단일 인스턴스 대신 workspace별 OS file lock과
lock owner/timeout/recovery 계약이 필요하다.

## 98. Node/Express에서 테스트 HTTP client 호출이 API로 중복 노출

테스트 대상은
`hagopj13/node-express-boilerplate@179ae84efec61b14206d0305d941daed6c6d07f9`
로 고정했다.

### 증상

UI에 API가 `232`개로 보였고, `tests/integration/auth.test.js`의
`request(app).post('/v1/auth/register')` 같은 Supertest 요청이 실제 서버
route처럼 반복됐다. 선택한 테스트 API는 HANDLES가 없었다.

### 근본 원인

1. JavaScript framework route rule이 `객체.method(path, ...)` 형태만 보고
   server router와 HTTP client를 구분하지 않았다.
2. route 추출이 framework activation 근거 파일의 소유권을 확인하지 않아
   테스트 파일까지 전역 스캔했다.
3. Koa pack은 `router.get`과 `app.use`라는 Express와 공유하는 두 신호만으로
   활성화되어 같은 route를 한 번 더 만들었다.
4. 제품 어댑터에는 test route를 차단하는 언어 독립적인 최종 방어가 없었다.

### 해결

- JavaScript/TypeScript의 registration route는 해당 framework의 identity를
  실제로 확인한 source file에서만 추출한다.
- import/require/package identity를 선언한 pack은 그중 하나가 확인되어야
  활성화되도록 공통 loader gate를 강화했다.
- 제품 adapter도 `is_test` 또는 `test/tests/__tests__` 경로의 Route를 API로
  공개하지 않는다.
- 테스트 client를 server route로 재분류하는 별도 예외 목록은 만들지 않았다.
  소유권과 identity라는 공통 원인을 한 번 수정했다.

### 결과

- UI/API 후보: `232 → 15`
- 테스트 파일 route: `32 → 0`
- 잘못 활성화된 Koa route: `43 → 0`
- Express만 활성화되고 실제 server route만 남음

## 99. Express chained route와 handler 선택 오류

### 문제

- `router.route('/').post(...).get(...)`는 method별 route가 아니라 `ANY` 한
  건으로 축약됐다.
- `router.post(path, validate(...), auth(), controller.create)`처럼 middleware가
  여러 개인 호출에서 첫 callback을 handler로 선택해 controller HANDLES가
  빠졌다.
- multiline 호출은 첫 줄만 읽어 마지막 handler와 정확한 source range를
  잃었다.

### 해결

- `.route(path)` 뒤의 chained verb를 각각의 route로 추출한다.
- 괄호 깊이를 추적해 최대 64줄까지만 bounded multiline call을 합친다.
- 최상위 인자를 파싱해 마지막 callback을 handler 후보로 선택한다.
- `*`는 화면과 그래프에서 일관되게 `/*`로 정규화한다.

### 결과

- 실제 Express route `15`개를 구분: 전역 OPTIONS `1`, `/v1/auth/*`,
  `/v1/docs`, `/v1/users`, `/v1/users/:userId`
- 확정 가능한 route의 HANDLES `4 → 14`
- 외부 `swaggerUi.setup` route 한 건은 로컬 handler로 거짓 연결하지 않음

## 100. JavaScript 짧은 이름 fallback의 교차 파일 오연결

### 문제

프로젝트 전체에서 handler의 짧은 이름만 찾는 fallback 때문에 외부
`swaggerUi.setup`이 관계없는 로컬 `bin/createNodejsApp.js/setup()`에 연결될
수 있었다. CommonJS `require('../../.../middlewares/...')`는 문자열에
`middleware`가 있다는 이유만으로 middleware fact가 되었고,
`.use('/path', routes)`에서는 path를 target으로 고르는 문제도 있었다.

### 해결

- JavaScript/TypeScript route handler는 동일 파일의 provider reference와
  source range 근거를 우선하며 프로젝트 전역 짧은 이름 fallback을 쓰지
  않는다.
- 같은 줄에 reference가 여러 개면 호출 대상에 가까운 오른쪽 reference를
  선택하고 실제 project definition symbol로 정규화한다.
- `require()` 경로 문자열은 middleware 선언 근거로 사용하지 않는다.
- `.use()`는 path가 아니라 마지막 target 인자를 middleware/router 후보로
  선택한다.

근거가 없는 외부 handler는 unresolved로 남긴다. 거짓 HANDLES보다 누락을
명시하는 것이 UI 신뢰성에 안전하다.

## 101. Express mount prefix가 최종 API 경로에 반영되지 않음

### 문제

개별 route 파일에는 `/register`만 있고 실제 `/v1/auth/register`는
`app.use('/v1', routes)`와 route table의 `{ path: '/auth', route: authRoute }`
조합으로 만들어진다. 테스트 오탐만 제거하면 UI에는 중복은 사라져도 잘못된
짧은 경로가 남을 수 있었다.

### 해결

CommonJS/ESM module binding과 Express `.use(prefix, router)`를 정적으로 따라가
고유한 nested mount prefix만 합성했다. route table의 `path`/`route` 쌍도
동일하게 처리한다. 결과에는 `localRoutePath`, `mountedRoutePath`,
`routePathSource=javascript-static-mount` 근거를 남긴다.

한 router가 여러 prefix에 mount되거나 binding이 모호하면 임의의 한 경로를
고르지 않고 local path를 유지한다. 동적 실행을 흉내 내는 JS interpreter는
추가하지 않았다.

## 102. 엔진 알고리즘 변경 후 framework cache가 과거 결과를 재사용

### 문제

언어 cache는 실행 파일 checksum을 key에 포함했지만 framework/architecture
cache는 수동 version 상수에만 의존했다. 엔진 코드를 다시 빌드해도 source가
같으면 과거의 중복 route와 잘못된 handler 결과를 재사용할 수 있었다.

### 해결

framework와 architecture cache key에도 현재 실행 파일 SHA-256을 포함했다.
수동 version bump를 빠뜨려도 분석 알고리즘이 바뀐 binary는 기존 결과를
재사용하지 않는다.

### 검증

동일 source와 동일 cache directory에서 새 binary로 재인덱싱했을 때
framework cache가 `cached=false`가 되고 새 key를 생성하는 것을 확인했다.

## 103. 살충제 패러독스를 줄이기 위한 테스트 조합

이번 수정은 한 fixture의 예상 개수만 고정하지 않고 서로 다른 실패 모드를
겹쳐 검사했다.

- 회귀 테스트: chained route, multiline final handler, 오른쪽 provider
  reference, mount prefix, test route 제품 필터
- 음성 테스트: Axios/Supertest client 호출은 route가 아님, Express syntax만
  있는 프로젝트에서 Koa는 활성화되지 않음, 한 개 신호/주석만으로 pack이
  활성화되지 않음
- metamorphic test: 같은 Supertest POST 100개와 GET 100개를 추가해도 실제
  Express route 집합이 변하지 않음
- ambiguity test: 여러 mount 후보는 추정하지 않고 fail-closed
- pack contract test: framework pack `84`종이 선언한 detection/extraction
  fixture를 계속 통과
- differential field matrix: 고정 커밋의 Spring(Java),
  CleanArchitecture(C#), FastAPI+TypeScript, Express(JavaScript)를 처음부터
  재인덱싱해 언어·framework·제품 adapter 결과를 비교
- end-to-end test: 앱에 번들한 release binary로 workspace 등록부터 제품
  inventory까지 다시 실행

최종 필드 결과:

- Spring Petclinic: engine `21 routes / 21 HANDLES`
- CleanArchitecture: product FastEndpoints `12 / 12`
- FastAPI+TypeScript: engine `39 / 39`
- Node Express boilerplate: engine `15 routes / 14 HANDLES`, product
  `15 routes / 14 HANDLES`, test route `0`, Koa route `0`

최종 gate:

- code engine unit tests `135/135`
- Tauri backend tests `248/248`, field tests `5`개는 명시적 ignored 후 실제
  저장소 환경에서 별도 실행
- frontend tests `149/149`
- engine/Tauri strict Clippy `-D warnings` 통과
- TypeScript production build와 dead-code gate 통과
- code contract smoke, evidence golden/negative gate, 4-repository field matrix
  통과
- bundled release engine checksum 및 development artifact manifest 검증 통과

## 104. 남은 정적 분석 경계

- 런타임에 문자열을 계산하거나 reflection/plugin으로 등록하는 route는 정적
  근거만으로 완전 복원할 수 없다.
- 같은 router를 여러 URL에 의도적으로 mount한 경우 현재 최소 구현은 경로를
  하나로 추정하지 않는다. 다중 endpoint 표현 계약이 필요해질 때 확장한다.
- import/require/package identity가 전혀 없는 잘린 코드 조각은 framework
  false positive를 막는 대신 false negative가 될 수 있다. 폴더 단위의 완전한
  프로젝트 분석에서는 manifest/import 근거를 사용한다.
- 외부 library callback은 로컬 handler로 위조하지 않으므로 HANDLES가 없는
  route가 정상적으로 존재할 수 있다. 이번 Express 저장소의 Swagger 문서
  route 한 건이 해당한다.

이 경계들은 임의 추정으로 숫자를 채우지 않는다. UI에서는 `미확인` 또는
직접 근거 없음으로 보여주는 것이 범용 프로젝트 신뢰성 계약이다.

## 105. Dart `package:` import가 framework identity 신호에서 제거됨

### 문제

문자열 내부의 route/API 예시를 코드 신호로 오인하지 않도록 import 문자열을
마스킹하는 과정에서 `import`와 `require` 계열만 인식하고 Dart의
`package:` 접두사는 인식하지 않았다. 따라서 `package:shelf/...`가 포함된
소스는 Shelf identity와 provider 연결이 누락될 수 있었다.

### 해결

공통 `signal_uses_string_literal` 경계에 `package` 접두사를 추가했다. 이제
Dart package import도 문자열 마스킹 대상이 되면서, import 자체는 보존된
framework 신호로 Shelf를 확인한다.

### 검증 및 효과

`dart_package_imports_can_confirm_shelf_identity` 회귀 테스트를 추가했다.
실제 `package:shelf/shelf.dart` import와 `Router().get(...)`가 있는 최소
소스에서 Shelf identity와 `/health` route가 동시에 확인된다. 문자열 안의
가짜 route는 계속 신호로 사용되지 않는다.

## 106. Next/SvelteKit/Nuxt 파일 기반 route가 method와 경로를 잃음

### 문제

파일 기반 라우팅은 일반 Express 호출과 달리 source에 명시적인
`router.get(...)`가 없을 수 있다. 기존 경로 복원은 다음 형태를 충분히
구분하지 못해 `ANY`, 잘못된 경로, 또는 빈 handler로 나타날 수 있었다.

- Next/SvelteKit의 `export const GET/POST = ...`
- Next route group `(admin)`와 optional catch-all `[[...slug]]`
- Nuxt `users/[id].get.ts` 같은 filename HTTP verb

이는 UI에서 route가 보이더라도 실제 method/path와 처리 관계가 틀리는
구조적 오류다.

### 해결

공통 file-system route parser가 export된 HTTP handler의 `GET`~`OPTIONS`를
찾고, Nuxt filename suffix를 method로 해석하도록 수정했다. Next/SvelteKit
route group과 parallel segment는 URL에서 제거하고, dynamic/catch-all/
optional catch-all은 `:id`, `*slug`로 정규화한다. source line과 handler
이름도 export 선언 위치를 사용한다.

### 검증 및 효과

`filesystem_routes_cover_exported_constants_groups_optional_catchalls_and_nuxt_suffixes`
테스트가 Next `/users/*slug` GET, SvelteKit `/` POST, Nuxt `/users/:id`
GET를 검증한다. 런타임 실행 없이도 파일 기반 framework의 핵심 route
계약이 framework별로 같은 규칙으로 유지된다.

## 107. 파일명 convention만으로 표시된 테스트 코드가 제품 API로 유입됨

### 문제

일부 provider는 `is_test` metadata를 주지 않고, 테스트 파일이 `tests/`
폴더에도 없을 수 있다. `auth.test.ts`, `auth.spec.ts`, `orders_test.go`,
`UserServiceTest.java`, `widget_spec.rb` 같은 파일은 기존 경로 기반 필터를
통과해 UI의 production API와 route 수를 부풀릴 수 있었다. Express 샘플의
중복 route 증상도 이런 경계가 약할 때 더 악화된다.

### 해결

제품 adapter의 공통 `is_test_file_path`에 언어별로 널리 쓰이는 파일명
convention을 추가했다. 반대로 `TestController.java`, `Contest.java`,
`testClient.go`처럼 우연히 문자열이 포함된 정상 파일은 오탐하지 않도록
stem 끝의 정확한 suffix와 시작 convention만 인정한다.

### 검증 및 효과

filename-only 테스트 6종과 정상 파일 3종을 분리한 단위 테스트를 추가하고,
product inventory fixture에도 `src/auth.test.ts` route를 넣어 제품 API에서
제외되는지 검증했다. provider가 metadata를 빠뜨려도 제품 경계에서 테스트
route가 재노출되지 않는다.

## 108. 최종 심층 검증 결과와 남은 release 경계

이번 추가 패치 이후 결과는 다음과 같다.

- engine unit tests `137/137`
- Tauri backend tests `249/249`, 명시적 ignored `5`
- frontend tests `149/149`
- engine/Tauri strict Clippy와 Rust format check 통과
- TypeScript production build 통과
- code contract smoke와 evidence golden/negative gate 통과
- release binary를 직접 지정한 4-repository field matrix 통과
- Spring `21/21`, FastEndpoints product `12/12`, FastAPI `39/39`, Express
  product `15 routes / 14 HANDLES`, test route `0`, Koa `0`

현재 `verify:release-engines`가 실패하는 것은 새 parser 버그가 아니라
manifest의 `codebase-memory.releaseReady=false`와 빈 release URL이 실제
공개 release가 아님을 정확히 차단하기 때문이다. 로컬 bundled executable은
release profile로 빌드되어 development artifact checksum으로 검증했다.
공개 release를 만들 때는 GitHub archive를 업로드한 뒤 archive/executable
checksum, URL, source commit을 갱신하고 `releaseReady=true`로 바꿔야 한다.
그 전에는 정식 release로 가장하지 않는 현재 동작이 안전하다.

## 109. 내부 release smoke fixture가 framework identity 근거 없이 Java route를 검사함

### 문제

새 framework identity gate는 `@GetMapping` 같은 annotation 이름만으로
Spring MVC를 활성화하지 않는다. 그런데 installer smoke fixture는 custom
annotation 선언만 포함하고 실제 `org.springframework.web` import가 없어,
설치된 엔진이 Java provider에서 `CALLS`는 반환해도 framework `HANDLES`를
생성하지 못했다. 이 실패는 provider bundle 추출 문제가 아니라 검증 fixture가
실제 프로젝트의 identity 계약을 만족하지 못한 것이었다.

### 해결

identity gate를 약화하지 않고 Java smoke fixture에 실제 Spring annotation
import를 추가했다. 이제 provider가 해석할 수 있는 구조와 framework identity
근거를 함께 가진 fixture만 release smoke를 통과한다.

### 검증 및 효과

수정 후 bundled engine contract와 설치된 installer 전체 smoke가 모두
통과했다. 설치 경로에서 Java `HANDLES` 관계가 확인되고, 앱 실행·격리
WebView/app-data·uninstall·registry cleanup까지 완료된다. 검증용 fixture가
제품 신뢰성 정책을 우회하지 않으므로 이후 identity gate 변경에도 smoke가
거짓 양성을 만들지 않는다.

## 110. 프로젝트 탐색기 하단 핵심 보기 영역이 상태바와 겹침

### 문제

프로젝트 탐색기는 `56px 48px 1fr 42px` 고정 grid 행을 사용하고 있었지만,
하단 footer에는 제목, 두 개의 버튼, gap, 상하 padding이 있어 실제 필요한
높이가 42px보다 컸다. 부모가 `overflow: hidden`이어서 마지막 MOD 항목이
footer와 상태바 아래로 잘려 보였다.

### 해결

footer grid 행을 고정 `42px`에서 `auto`로 변경했다. 이제 footer는 실제
콘텐츠 높이를 차지하고, tree 영역만 남은 높이를 사용해 내부 스크롤한다.

### 검증 및 효과

`npm test -- --run` `149/149`, `npm run build`를 통과했다. 항목 수가 많은
프로젝트에서도 마지막 tree 항목이 하단 action footer에 가려지지 않고,
작은 창에서는 tree만 스크롤되는 레이아웃 계약이 유지된다.

## 111. 코드 탐색기가 평면 목록이라 디렉토리 구조를 이해하기 어려움

### 문제

API 탐색기는 URL path를 폴더처럼 보여주지만 코드 탐색기는 모든 handler,
service, function, class, module, file을 한 목록에 나열했다. 파일 경로가
항목의 작은 메타 텍스트에만 있어, 같은 이름의 심볼이 여러 디렉토리에 있을
때 소속과 구조를 빠르게 구분하기 어려웠다.

### 해결

코드 대상에 원본 `sourcePath`를 보존하고, 언어·프레임워크별 폴더 규칙이
아닌 실제 파일 경로를 공통 `buildCodeTree`로 분할했다. 프로젝트 화면과
코드 탭 모두 `디렉토리 -> 파일 -> 코드 항목` 계층을 사용하며, 디렉토리와
파일 노드는 접기/펼치기, 하위 항목 수, 파일 아이콘을 제공한다. 코드 항목
선택은 기존 `search-focus`와 focus ID를 그대로 사용하고, 500개 상한과
검색 동작도 유지했다. 폴더 버튼에는 명시적인 접근성 이름도 추가했다.

### 검증 및 효과

경로 트리·Windows 경로 정규화·소스 위치 없음·언어 독립 grouping 테스트와
실제 컴포넌트 선택 테스트를 추가했다. 전체 frontend 테스트는 `151/151`,
production build는 통과했다. API/DB 탐색과 기존 footer 레이아웃 변경에는
회귀가 없으며, 동일한 코드 이름이 다른 디렉토리에 있어도 경로별로
구분되어 표시된다.

## 112. API 연결 지도에서 주 경로 외 관계가 접힌 목록에만 표시됨

### 문제

API 연결 지도는 `primaryPath` 하나만 큰 카드와 선으로 보여주고, 같은
핸들러에서 다른 함수로 이어지는 `CALLS`나 추가 DB 관계는 `+N 연결`을
눌러야 목록에서 확인할 수 있었다. 관계 데이터가 없는 것이 아니라 UI
투영 단계에서 여러 실제 edge를 주 시각화에서 숨기고 있었다.

### 해결

분석 엔진과 관계 판정은 변경하지 않고, 기존 `map.edges`의 모든 관계를
하나의 연결 캔버스에 투영했다. `primaryPath`는 상단의 주 흐름으로
고정하고, 주 경로 밖의 노드는 실제 edge 방향을 따라 아래 분기 행에
배치한다. 모든 관계를 SVG 화살표와 라벨로 연결해 `Handler -> 여러
Function` 같은 fan-out을 별도 목록 없이 동시에 볼 수 있게 했다. 노드는
기존 Inspector 선택으로, 관계 라벨은 기존 근거 선택으로 연결했다. 확정
관계와 후보 관계는 실선/점선과 색으로 계속 구분하며, 노드가 누락된 edge는
추측하지 않고 그래프에서 제외한다.

### 검증 및 효과

통합 그래프의 추가 노드 선택, 관계 근거 선택, 다중 fan-out 배치를
컴포넌트 테스트로 고정했다. 전체 frontend 테스트는 `153/153`, production
build는 통과했다.
이제 한 캔버스에서 주 경로와 실제 분기 연결을 함께 파악할 수 있으며,
기존의 확정/후보 신뢰도 경계를 훼손하지 않는다.

## 113. 지원 언어 계약은 14개인데 실제 bridge와 framework pack은 12개

### 문제

제품 지원 계약은 Kotlin과 Swift를 포함한 14개 언어를 공식 지원 대상으로
기록하고 있지만, 현재 `code_memory/rust/src/model.rs`의 language bridge와
`code_memory/packs/framework` catalog는 TypeScript, JavaScript, Python,
Java, C#, C, C++, Go, Rust, PHP, Ruby, Dart 12개만 구현한다. 현재 language
semantic gate도 이 12개만 실행한다. 문서의 지원 언어 수와 실제 provider·pack·CI
범위가 다르면 사용자는 Kotlin/Swift에도 동일한 API·호출·DB 품질이 보장된다고
오해할 수 있다.

### 근본 원인

지원 범위를 문서, bridge registry, framework catalog, product validation,
release gate가 각각 독립적으로 표현한다. 또한 현재 framework fixture gate는
pack fact와 source evidence를 검증하지만 모든 언어의 route → 내부 호출 → DB
완주 품질을 공통으로 검증하지 않는다.

### 해결 방향

현재 active 언어와 target 언어를 분리하고, active 언어 전체에 동일한 uniform
core quality gate를 적용해야 한다. 최소 공통 기준은 파일 coverage, 심볼,
cross-file direct `CALLS`, caller/callee stable ID, source range, 중복 제거,
provider diagnostic, name-only 확정 금지다. Kotlin/Swift는 provider와 같은
gate가 생기기 전까지 active supported로 승격하지 않는다. framework와 ORM은
그 위의 capability conformance fixture로 별도 승격한다.

이번 단계에 추가한 자료:

- `visual_map/code_memory/docs/contracts/UNIFORM-CORE-QUALITY.md`
- `visual_map/code_memory/tests/gates/run-uniform-core-quality-gate.ps1`
- `visual_map/docs/research/uniform-language-quality.md`
- `visual_map/docs/plans/uniform-language-quality.md`

## 114. Rust가 오류 없이 indexed인데 직접 호출 관계가 0개가 되는 조용한 부분 성공

### 문제

Rust semantic fixture에서 provider 오류나 실패 상태 없이 `indexed`가 반환됐지만,
작은 workspace의 cross-file `CALLS`가 0개였다. 기존 LSP reference 보강은 Ruby와
환경변수로 강제한 경우에만 켜져 있어 Rust call hierarchy가 충분하지 않은 환경에서
호출 근거가 누락될 수 있었다. 이 상태는 UI에서 “분석 완료”처럼 보이지만 실제 흐름은
끊기는 신뢰성 문제다.

### 근본 원인

provider 상태(`indexed`)와 핵심 의미 coverage(`CALLS`)가 별도 계약으로 검사되지
않았고, Rust reference enrichment가 기본 활성 언어가 아니었다. 즉 provider가
실행됐다는 사실을 분석 결과가 충분하다는 뜻으로 잘못 해석할 여지가 있었다.

### 해결

공통 LSP 경로에서 Rust도 기본 reference enrichment 대상으로 변경하고, Rust 회귀
테스트를 추가했다. 또한 12개 활성 언어 모두에 대해 문서·관계 endpoint·CALLS·source
range·중복·오류 진단을 동일하게 검사하는 `run-uniform-core-quality-gate.ps1`를
추가했다. fixture 목록, bridge registry, framework catalog의 언어 집합이 달라져도
게이트가 즉시 실패한다. CI에서도 release bridge 빌드 직후 이 게이트를 실행한다.

### 검증 및 효과

`cargo fmt --check`, Rust 테스트 `138/138`, release build, 기존 semantic gate
`12/12`, 새 uniform core gate `12/12`를 통과했다. 이제 “provider가 실행됨”만으로
완료를 주장하지 않고, 모든 active 언어가 동일한 최소 의미 품질을 통과해야 CI가
통과한다. 이는 route-to-DB 전체 완성도를 보장하는 최종 단계가 아니라, 그 위에
framework/ORM flow conformance를 쌓기 위한 공통 바닥 계약이다.

Ruby에서는 Windows Bundler platform 경고가 출력되지만 현재 fixture의 결과와
uniform gate는 통과한다. release 전에는 이 경고를 정상/부분 성공/실패로 분리하는
provider diagnostic 정책을 다음 단계에서 다뤄야 한다.

## 115. 코드 관계 gap이 실행 순서에 의존하고 endpoint 연결도 깨짐

### 문제

코드 관계 gap은 \`CodeInventoryGap\`에 자체 ID가 없어서 snapshot에서 배열 index를
ID에 넣었다. 같은 프로젝트를 다시 읽거나 provider 결과 순서가 달라지면 동일한
문제가 다른 ID가 됐다. 또한 snapshot의 \`related_ids\`에 한 endpoint는 \`code:\`
접두사를 붙이고 다른 endpoint는 원본 ID를 그대로 넣어, 실제 코드 항목과 연결되지
않는 gap이 생겼다. provider 진단처럼 실제 코드 항목이 없는 global gap도 관련 ID가
가짜로 채워져 API 화면에서 누락될 수 있었다.

### 해결

\`CodeInventoryGap\`에 내용(kind/from/to/message) 기반의 결정적 SHA-256 ID를 추가하고,
기존 ID가 없는 legacy 결과도 같은 방식으로 보정한다. snapshot은 이 ID를 그대로
사용하며, 실제 snapshot item으로 존재하는 endpoint만 \`code:<id>\` 형태로
\`related_ids\`에 연결한다. 알려진 endpoint가 하나도 없으면 related ID를 비워
global provider gap으로 API/근거 화면에 노출한다. 코드 adapter 버전을 5로 올려
기존 snapshot은 자동으로 stale/reindex 상태가 되게 했다.

### 검증 및 효과

stable gap ID와 알려진 endpoint 연결을 고정하는 Tauri 회귀 테스트를 추가했다.
Tauri 테스트 \`250 passed, 0 failed, 5 ignored\`, Clippy \`-D warnings\`, format,
frontend runtime contract 테스트, diff check를 통과했다. 이제 provider 진단과
unresolved 관계가 재실행 순서나 배열 위치에 따라 다른 문제로 보이지 않고,
API가 실제 관련 code node와 global coverage gap을 구분할 수 있다.

## 116. 2026-08-01 — 84개 framework provider 동일 품질 게이트에서 발견한 불일치

### 증상

모든 active 언어의 core gate는 통과했지만, framework provider를 언어별로 실제
index하는 게이트를 처음 돌리자 C++ Crow, Dart metadata, Go import signal, Java
fixture/dedupe, Angular, Vue, Fastify, API Platform, Django route에서 서로 다른
품질 문제가 드러났다. 첫 실행은 `python/django HTTP_ROUTE`가 경로는 만들었지만
handler symbol을 확정하지 못해 실패했다.

### 근본 원인

provider 자체의 공통 계약보다 pack signal, source masking, fixture 형태, generic
fact evidence, 등록형 route 인자 해석이 서로 어긋나 있었다. 특히 Django의
`path("/fixture", handler)`는 첫 번째 인자인 경로만 인식되고 두 번째 handler
인자는 route end 이후에 있어 기존 공통 parser가 읽지 못했다.

### 해결

84개 pack을 허용 목록 없이 검사하는 `run-framework-provider-gate.ps1`를 추가하고
CI에 연결했다. signal은 masking 규칙에 맞게 정규화했고, false-positive fact는
framework 의미가 있는 declaration에서만 생성하도록 제한했다. 등록형 route에는
Django `path/re_path`와 Starlette `Route`의 두 번째 callable 인자를 공통 parser로
해석하고, 회귀 테스트를 추가했다.

Ruby LSP의 Bundler 플랫폼 불일치는 결과를 성공으로 숨기지 않고 warning 진단으로
보존한다. provider가 반환한 문서와 관계는 유지하되 프로젝트 gem 해석이 불완전할
수 있음을 표시하는 현재의 fail-safe 동작이다.

### 검증 및 효과

수정 후 framework provider gate는 `84 passed, 0 failed`, Rust engine 테스트는
`139 passed, 0 failed`이다. 이제 새 pack/provider가 탐지되기만 하고 source range,
resolved symbol, route ownership을 놓치는 경우 CI에서 차단된다. 단, 이 게이트가
모든 프로젝트의 route → service → repository → DB 완전 추적을 의미하지는 않으며,
그 부분은 framework/ORM cross-file flow와 exact DB join 게이트의 다음 범위다.

## 117. 2026-08-01 — framework 감지 성공과 실제 내부 호출 성공을 혼동할 수 있음

### 문제

84개 framework provider gate는 pack의 fact, source range, route ownership은 검사하지만,
pack이 감지된 뒤 handler에서 실제 service까지 `CALLS`가 이어지는지는 공통으로 검사하지
않았다. 그래서 provider가 `detected`를 반환해도 UI의 선택 API 흐름은 두 노드에서 끊길 수
있었다.

### 해결

12개 active 언어의 대표 framework fixture를 `엔트리포인트 → handler → service` 공통
flow gate로 만들고, 각 단계의 symbol, source path/range, service document, 중복 없는
CALLS를 검사했다. Java project metadata 누락, Rust 겹치는 호출 range 중복, PHP method
endpoint 형태, Ruby 괄호 없는 method call을 실제 실패로 재현하고 근본 원인을 수정했다.
CI에 이 게이트를 framework provider gate 다음 단계로 연결했다.

### 검증 및 효과

`run-framework-flow-gate.ps1`: `12/12` 통과. 이제 framework가 보인다는 사실만으로
흐름 완성을 주장하지 않고, 모든 active 언어가 같은 최소 내부 호출 계약을 통과해야 한다.
서로 다른 호출 위치는 유지하고 같은 호출의 겹치는 provider range만 하나로 합친다.

## 118. 2026-08-01 — 언어별 SQL 호출 문법이 DB evidence에서 빠질 위험

### 문제

DB exact join은 정적 SQL 실행 근거와 DB snapshot의 정확한 table/column 매칭이 모두
필요한데, 기존 회귀 테스트가 일부 언어/framework 문법만 확인했다. C 계열 native
`sqlite3_exec`처럼 SQL이 두 번째 인자인 호출이나 PHP PDO `->query`는 공통 실행식
판별 기준에서 빠질 수 있었다.

### 해결

공통 semantic linker에 bounded execution marker를 추가했다. C/C++ native SQL 호출은
알려진 전역 함수와 인자 위치를 확인하고, PHP PDO/DBAL 계열은 receiver whitelist를
통과할 때만 인정한다. 12개 active 언어의 대표 정적 SELECT를 같은 `READS`와 exact
column evidence 규칙으로 검증하는 단위 게이트를 추가했다. `logger->query` 같은
일반 객체 호출과 동적 SQL은 계속 확정하지 않는다.

### 검증 및 효과

Tauri semantic linker conformance test가 12개 언어 모두 통과했다. 이 테스트는 언어별
문법을 허용하되 결과 계약은 하나로 유지한다. 다음 단계는 이 공통 evidence를 실제
DB snapshot의 exact/ambiguous/missing/stale 상태와 묶어 API flow와 DB 화면까지 같은
gap 상태로 투영하는 것이다.

## 119. 2026-08-01 — 실제 Spring Petclinic/Node Express 회귀 결과와 정상적인 gap 구분

### 검증

사용자가 앞서 제시한 두 공개 프로젝트를 별도 임시 디렉터리에 shallow clone하고,
현재 release bridge로 원본을 수정하지 않은 채 인덱싱했다.

- `spring-petclinic-microservices`: Java 62/62 파일 indexed, 문서 84개, Spring 계열
  4개 pack detected, HTTP route 16개, HANDLES 16개, CALLS 148개
- `node-express-boilerplate`: JavaScript 39/39 파일 indexed, Express detected,
  framework fact 35개, route 15개, HANDLES 14개, CALLS 185개
- 두 결과 모두 duplicate document 0, 동일 endpoint/path/range CALLS duplicate 0

### Node 프로젝트에서 확인된 미확정 1건

`GET /v1/docs`는 route fact는 생성되지만 handler symbol과 HANDLES가 없다. 원본이
`swagger-ui-express`의 `serve/setup`을 동적으로 등록하기 때문에 외부 라이브러리
호출을 프로젝트 내부 handler로 이름만 보고 확정하면 오탐이다. 따라서 이 결과는 버그가
아니라 `unresolved` gap으로 남겨야 하며, UI에서 “DB나 handler가 없다”고 단정하지
않고 “직접 근거를 찾지 못함”으로 설명해야 한다.

### 경고 해석

Spring 결과의 Java deprecation/raw-type warning과 Node 결과의 test 파일
project-config 제외는 분석 실패가 아니다. provider가 반환한 문서와 관계는 유지하면서
warning/coverage gap을 진단으로 보존하는 현재 정책이 맞다. 다만 release 품질 화면에서는
`indexed`, `indexed-partial`, `warning`, `unresolved`를 하나의 “완료”로 뭉치지 않도록
분리해서 보여줘야 한다.

## 120. 2026-08-01 — 엔진 상태와 코드 화면의 품질 표시가 분리될 위험

### 문제

엔진은 언어별 provider 상태와 framework adapter 상태를 계산하고 있었지만, Tauri가
전달하는 architecture payload에는 nodes/edges/diagnostics만 있어 코드 화면에서 어떤
언어가 정상인지, 어떤 언어가 부분인지, framework fact가 실제로 몇 개인지 공통 기준으로
보여줄 수 없었다. 이 상태에서는 분석이 부분 실패해도 단순히 “코드 읽힘”으로 보일 수 있다.

### 해결

architecture payload를 `code-memory.architecture-index.v2`로 올리고 `languages`와
`frameworks` 요약을 추가했다. 언어에는 provider, 파일 발견/색인/제외/누락 수, status를,
framework에는 adapter, status, fact/relation 수를 담는다. 코드 패널은 이 공통 값을
`정상/부분/확인 필요`로 표시하고 기존 `partial`과 gap을 숨기지 않는다. 새 payload를
구분하기 위해 architecture cache key도 갱신했다.

### 검증 및 한계

엔진 quality summary unit test, TypeScript typecheck, frontend 155개 테스트, Rust
143개 테스트가 통과했다. 이 변경은 상태를 정직하게 노출하는 공통 기반이며, 아직 84개
pack 각각의 고급 flow와 Project/API/DB 전체를 하나의 capability matrix로 펼치는 작업은
남아 있다. 미확정 관계를 정상으로 승격시키는 우회는 추가하지 않았다.

## 121. 2026-08-01 — Spring 다중 annotation constructor DI가 다음 메서드에 잘못 귀속됨

### 재현

실험장 `D:\visual_map_reliability_lab\java-spring-petclinic-microservices`에서
`GET /owners/{ownerId}`를 재색인했다. route와 `OwnerResource.findOwner`의 HANDLES는
정확했지만, `OwnerResource(OwnerRepository, OwnerEntityMapper)` constructor DI가
처음에는 누락되거나 다음 `createOwner` 메서드의 dependency처럼 보였다.

### 근본 원인

Java constructor injection 판별기가 클래스 선언 바로 위의 첫 번째 non-empty annotation만
검사했다. Petclinic처럼 `@RequestMapping`, `@RestController`, `@Timed`가 연속된 경우
첫 번째로 만난 `@Timed`가 대상 annotation 목록에 없으면 즉시 중단했다. 또한 dependency
fact의 symbol을 `nearby_handler`로 찾으면서 constructor 뒤의 첫 public method인
`createOwner`에 잘못 귀속할 수 있었다.

### 해결

공통 Java framework 계층에서 연속된 annotation block 전체를 검사하고, constructor DI의
소유자를 주변 메서드가 아니라 enclosing class symbol로 확정하도록 수정했다. 특정
Petclinic 파일명이나 메서드명을 추가하지 않았다. 회귀 테스트는 여러 annotation과 두 개
constructor dependency를 가진 일반 `@RestController` fixture로 고정했다.

### 검증

수정 후 Petclinic 결과는 Java 62/62, JavaScript 22/22 indexed, Spring pack 4개,
HTTP route 16개, HANDLES 16개, CALLS 148개, duplicate CALLS 0개다. `/owners/{ownerId}`는
HANDLES 1개, `OwnerRepository`/`OwnerEntityMapper` dependency 2개가 모두
`OwnerResource` class symbol에 연결된다.

### 남은 의도적 gap

`OwnerRepository.findById`는 프로젝트 인터페이스에 선언된 메서드가 아니라 외부
Spring Data `JpaRepository`에서 상속된 런타임 구현이다. 따라서 현재 provider가 가진
프로젝트 내부 exact symbol/range만으로 repository method나 실제 SQL/DB table을 확정하면
오탐이다. 이 지점은 Spring Data/JPA capability를 별도 공통 adapter로 인증할 때
`OwnerRepository → inherited method → entity/table`의 bounded 규칙을 추가해야 하며,
그 전까지는 미확정으로 표시하는 정책이 맞다.

## 122. 2026-08-01 — 외부 실험 clone의 untracked project config가 분석 범위를 바꿈

### 관찰

`D:\visual_map_reliability_lab\java-spring-petclinic-microservices`에는 원격 저장소에
없는 untracked `tsconfig.json`이 이미 있었다. 이 파일 때문에 Java 62개뿐 아니라
정적 JavaScript 22개도 분석 대상이 되어 결과 language status가 JavaScript까지 포함됐다.

### 판단

이는 엔진이 Java 파일을 잘못 JavaScript로 분류한 문제가 아니라, 프로젝트에 존재하는
설정 파일을 기준으로 분석 범위를 넓힌 정상 동작이다. 다만 동일 레포를 재현 비교할 때
clone 상태가 달라지면 language/file coverage가 달라질 수 있다.

### 처리

사용자 파일을 삭제하거나 수정하지 않았다. 외부 field test는 `git status`와 원격 기준을
먼저 기록하고, untracked 설정은 별도 실험 입력으로 표시해야 한다. release 품질 비교는
깨끗한 clone과 사용자 설정이 있는 clone을 나눠 실행한다.

## 124. 2026-08-01 — C# Minimal API custom endpoint group의 route가 전부 누락됨

### 재현

D:\visual_map_reliability_lab\cleanarchitecture는 일반적인
app.MapPost("/path", handler)가 아니라 자체 extension overload인
groupBuilder.MapPost(handler, "optional-path")를 사용한다. 또한
IEndpointGroup 구현을 reflection으로 찾은 뒤 MapGroup("/api/{GroupName}")에
연결한다. 최초 색인에서는 ASP.NET Core dependency fact만 생성되고 HTTP route는
0개였다.

### 근본 원인

기존 Minimal API pack의 감지 신호가 app.MapGet 같은 receiver 고정 형태에만
의존했고, route parser도 path-first 호출만 처리했다. 따라서 groupBuilder.MapPost
호출은 framework 자체가 활성화되지 않았고, 활성화하더라도 handler와 path의
argument 순서를 읽을 수 없었다. 반대로 모든 MapX 문자열을 무조건 route로
인정하면 extension wrapper의 builder.MapGet(pattern, handler) 구현부까지 실제
endpoint로 오인하는 문제가 생긴다.

### 해결

Minimal API 공통 parser가 path-first와 handler-first overload를 구분하고, handler-first
형태에서는 두 번째 인자가 정적 문자열일 때만 path로 확정한다. 동적 변수 path는
추측하지 않으며, wrapper처럼 두 번째 인자가 일반 변수인 호출은 route로 만들지 않는다.
또한 IEndpointGroup reflection discovery, MapGroup, RoutePrefix, fallback
template가 실제 source에 함께 있을 때만 group prefix를 합성한다. 특정 endpoint
클래스 이름이나 파일 경로를 하드코딩하지 않았다.

### 검증

회귀 테스트 2개를 추가하고 Rust 전체 테스트 147개, 엄격 Clippy, 포맷 검사를 통과시켰다.
84개 framework provider gate, 12개 cross-language flow gate, 12개 uniform core quality
gate도 모두 통과했다. field index 결과는
ASP.NET Minimal API route 10개, HANDLES 10개, wrapper 오탐 0개이며 실제 경로는
/api/TodoLists, /api/TodoItems/{id}, /api/Users/logout,
/api/WeatherForecasts 형태로 복원됐다. 결과 파일은
D:\visual_map_reliability_lab\_results\cleanarchitecture.after-minimal-v3.json와
동일 이름의 .architecture.json이다.

### 별도 외부 의존성 경고

원본 레포의 dotnet restore는 TreatWarningsAsErrors 때문에 MessagePack,
System.Security.Cryptography.Xml, Microsoft.OpenApi, OpenTelemetry, SQLite 계열
패키지 advisory를 오류로 보고했다. 이는 분석기 route 버그와 별개의 레포 의존성
상태이며, 실험 clone을 수정하거나 경고를 숨기지 않았다. SCIP는 151개 C# 문서를
생성했고 분석 결과에는 부분 provider 진단으로 보존됐다.

### 남은 핵심 gap

endpoint에서 ISender.Send(new CreateTodoListCommand())까지의 command 참조와
각 IRequestHandler<TRequest> 구현은 provider 관계에서 각각 보이지만, MediatR의
runtime dispatch 때문에 endpoint → 구체 handler를 현재 CALLS로 확정하지 않는다.
또한 handler의 EF Core DbSet/SaveChangesAsync는 보이지만 실제 route flow의 DB
table까지 자동 확정하지 않는다. 다음 공통 개선은 Send(request type) →
IRequestHandler<request type>를 명시적 DISPATCHES 상태로 연결하고, EF Core
entity/table mapping이 실제 source 근거를 가질 때만 DB node를 확정하는 것이다.

## 123. 2026-08-01 — FastAPI 중첩 router의 전역 prefix가 사라짐

### 재현

`D:\visual_map_reliability_lab\fastapi-full-stack-fastapi-template`를 원본 그대로
색인했을 때 실제 애플리케이션 설정은 `app.include_router(api_router,
prefix=settings.API_V1_STR)`, `API_V1_STR = "/api/v1"`, 하위 router는
`prefix="/items"`였지만 route fact가 `/items`처럼 local path로만 생성됐다. 이
상태에서는 API 화면의 endpoint와 HANDLES가 실제 호출 URL과 달라진다.

### 근본 원인

기존 FastAPI adapter가 다음 네 가지를 각각 처리하지 못했다.

1. `from app.api.routes import items`에서 package `__init__.py`를 먼저 선택해
   `items.py`를 찾지 못했다.
2. `items.router`를 하나의 alias로 취급해 import alias와 router member를 분리하지
   못했다.
3. `prefix=settings.API_V1_STR`처럼 문자열이 아닌 prefix 표현식을 읽지 못했다.
4. prefix 설정이 route 후보 파일 밖의 `core/config.py`에 있어 candidate source만
   넘기면 정적 설정값을 볼 수 없었다. 또한 부모 mount prefix와 자식 router prefix를
   후보 중 하나로 고르는 방식이라 두 prefix를 합성하지 못했다.

### 해결

공통 FastAPI route context가 전체 Python source를 읽어 정적 문자열 설정을 해석하고,
package import는 symbol 파일(`items.py`)을 우선 선택하며, dotted child를
`alias + member`로 분리한다. mount 경로는 `부모 prefix + include_router prefix +
자식 APIRouter prefix` 순서로 합성하고, 여러 경로가 가능한 경우에는 임의의 경로를
확정하지 않는다. 특정 레포 파일명이나 `/api/v1` 값을 하드코딩하지 않았다.

### 검증

FastAPI adapter 회귀 테스트 2개와 release build를 통과했다. field index 결과는
Python 47/47, TypeScript 95/95 indexed, FastAPI route 23개 모두 실제 전역 경로를
포함하고, HANDLES 23개와 INJECTS 10개가 생성됐다. 결과 파일은
`D:\visual_map_reliability_lab\_results\fastapi-full-stack.after-prefix-v3.json`와
동일 이름의 `.architecture.json`이다.

### 남은 실제 gap

이 수정으로 URL 경로는 해결됐지만 “route에서 DB까지 완성된 흐름”이 모두 해결된 것은
아니다. `SessionDep`/`CurrentUser` 같은 `Annotated[..., Depends(...)]` alias의
사용 위치를 route handler dependency edge로 UI inventory가 아직 투영하지 않으며,
SQLModel의 `session.exec/get/add/commit`은 정확한 table을 추측하지 않도록 현재 DB
관계로 확정하지 않는다. 또한 module-level architecture flow는 같은 모듈의 여러
route를 한 흐름에 섞을 수 있어, symbol-level focused flow가 다음 공통 개선 대상이다.
이 세 가지를 근거 없이 확정 관계로 표시하는 것은 현재보다 더 위험한 오탐이므로,
다음 단계에서 `dependency alias → handler`, `ORM operation → READ/WRITE/unknown`,
`symbol-level flow`를 각각 명시적 상태와 함께 추가해야 한다.

## 125. 2026-08-01 — SimpleBank Go Gin handler 충돌과 gRPC endpoint 과검출

### 재현

`D:\visual_map_reliability_lab\simplebank`의 원본 commit은
`97f000f feat: add token type to token payload (#136)`이다. Go 77개 파일 중
76개가 색인됐고, Gin route는 8개가 생성됐지만 처음에는 `loginUser`와
`renewAccessToken` 두 개만 HANDLES로 확정됐다. `createUser`, `createAccount`,
`getAccount`, `listAccounts`, `createTransfer`는 모두 미확정이었다.

동시에 gRPC adapter는 실제 서비스 등록과 무관한 `grpc.NewServer`, interceptor,
reflection, generated helper 주변까지 RPC endpoint로 잡아 RPC_ENDPOINT 101개와
HANDLES 57개를 생성했다. 이는 실제 서비스 수가 아니라 문자열 신호가 반복된 결과다.

### 근본 원인

1. Gin의 `router.POST("/users", server.createUser)`에서 기존 Go handler resolver가
   `createUser`라는 짧은 이름만 프로젝트 전체에서 찾았다. sqlc generated code에도
   같은 소문자 query variable이 있어 `(*Server).createUser`와 충돌했고, 모호한
   대상을 확정하지 않는 안전 정책 때문에 route가 미확정으로 남았다.
2. gRPC의 공통 `RPC_ENDPOINT` evidence가 Go에서도 `grpc`, `Register` 같은 넓은
   문자열을 모든 줄에 적용했다. 등록 API 호출과 단순 import/초기화/로그 코드를
   구별하지 못했다.
3. SimpleBank의 `main()`은 `runGinServer`를 호출하지 않고 `runGatewayServer`와
   `runGrpcServer`를 실행한다. 따라서 Gin `/transfers`는 코드와 테스트에 존재하는
   route이지, 현재 main runtime에서 활성화된 운영 entrypoint라고 단정할 수 없다.

### 해결

Go registration resolver가 `server.method` 형태의 등록식과 enclosing Go receiver
타입을 함께 읽어 `(*Server).createTransfer`처럼 정확한 method symbol을 우선한다.
동명 generated symbol은 receiver 타입이 맞지 않으면 선택하지 않는다.

Go gRPC `RPC_ENDPOINT`는 `RegisterService(...)` 또는 `Register…Server(...)` 형태만
확정 evidence로 제한했다. 단순 `grpc.NewServer()`, `reflection.Register(...)`,
import 및 interceptor 줄은 endpoint가 아니다. 저장소 파일명이나 handler 이름을
하드코딩하지 않았다.

### 검증

수정 후 field index는 Go 76/76 indexed다. Gin은 실제 운영 코드의 7개 route를
모두 handler에 연결했다.

`POST /users → (*Server).createUser`, `POST /users/login → (*Server).loginUser`,
`POST /tokens/renew_access → (*Server).renewAccessToken`,
`POST /accounts → (*Server).createAccount`, `GET /accounts/:id →
(*Server).getAccount`, `GET /accounts → (*Server).listAccounts`,
`POST /transfers → (*Server).createTransfer`가 확정됐다. 테스트에서 등록한
익명 `GET /auth`만 의도적으로 미확정이다.

gRPC RPC_ENDPOINT는 101개에서 5개로 줄었고, registration boundary에 대한
HANDLES 5개와 ASYNC_CALLS 1개만 남았다. 결과 파일은
`D:\visual_map_reliability_lab\_results\simplebank.after-go-adapters-v2.json`와
동일 이름의 `.architecture.json`이다.

엔진 검증은 Rust 테스트 149개, 엄격 Clippy, fmt, framework provider 84/84,
cross-language flow 12/12, uniform core quality 12/12를 모두 통과했다.

### 외부 프로젝트 테스트 결과

`go test ./...`에서 API, gapi, token, util 등은 통과했다. `db/sqlc` 테스트는
필요한 PostgreSQL `localhost:5432`가 실행 중이지 않아 connection refused로
실패했고, `mail`의 Gmail SMTP 테스트는 535 BadCredentials로 실패했다. 이는
분석기 변경이나 SimpleBank clone의 소스 수정이 아닌 외부 실행 환경/자격증명
문제이므로 비밀값을 저장하거나 clone을 수정하지 않았다.

frontend의 JavaScript/TypeScript는 `@vue/tsconfig/tsconfig.dom.json`과
`@tsconfig/node20/tsconfig.json` 의존성이 없는 깨끗한 clone 상태라 provider가
부분 실패로 기록됐다. Go 분석 결과와 섞어 성공으로 표시하지 않았다.

### 남은 핵심 gap

gRPC registration fact가 현재는 generated `RegisterSimpleBankServer`와
`ServiceDesc`까지는 보이지만, `Register…Server(server)`에서 실제 `gapi.Server`의
`CreateUser/LoginUser/VerifyEmail/UpdateUser` 구현 method까지 concrete dispatch로
연결하지 않는다. generated descriptor와 Go interface implementation을 함께 읽는
공통 `DISPATCHES` 관계가 다음 단계다.

또한 `POST /transfers` handler에서 `TransferTx → CreateTransfer/CreateEntry/
AddAccountBalance → pgx.QueryRow` 호출은 보이지만, repo의 17개 `.sql` query source와
generated sqlc method를 연결하지 않아 실제 table/SQL statement까지 확정하지 않는다.
SQL provider 또는 sqlc mapping adapter가 source 근거를 만들기 전에는 DB table을
추측하지 않는 것이 맞다. 마지막으로 Gin route는 runtime reachability를 분석해
`defined`, `test-only`, `active-from-main`, `unreachable` 상태를 분리해야 한다.

## 126. 2026-08-01 — 8개 다언어 현장 레포 공통 회귀 점검

### 검증 대상과 고정 commit

재현성을 위해 다음 레포를 `D:\visual_map_reliability_lab`에 shallow clone하고 각
HEAD를 기록했다.

| 언어/프레임워크 | 레포 | commit | 분석 범위 |
|---|---|---|---|
| Rust/Rocket | vaultwarden | `2629bcbe1380c894e3a7f52cafcac3988edb8fbb` | 전체 |
| TypeScript/NestJS | nestjs-boilerplate | `549cc37a3925ab87a4e61b45efb3b86d2d8e234e` | 전체 |
| PHP/Laravel | bagisto | `95872d4c101b1e6ea138780a73972ca25b314793` | 전체 |
| C++/Drogon | my-fastest-drogon-app-cpp | `4afafe03183f4036ad05ff4074e188de34eba388` | 전체 + 수동 compile DB |
| Dart/Serverpod | serverpod | `55cea3afa41614ab2348feaf04002387d40dad13` | `examples/auth/auth_server` |
| C/GMP | gvmd | `15c5c8d841da21a4cd6786d679fd920063450d90` | 전체 |
| JavaScript/Express | Ghost | `ee5529727040f3863682b7c8aa8aef70d4fbc20a` | `ghost/core` |
| Ruby/Rails | Discourse | `7d635130bfcf20154c5fe6b613777ed481f3cd53` | 전체 |

분석은 bundled providers와 packs를 명시해 수행했다. 결과 파일은 각 레포의
`D:\visual_map_reliability_lab\_results`에 저장했다.

### 공통 문제 A — SCIP 메서드와 TypeScript parameter 심볼 중복

NestJS route handler가 `login()` 메서드가 아니라 `login().(loginDto)` 같은
parameter child symbol로 선택될 수 있었다. 공통 indexed definition 선택기에서
`().(` 형태의 parameter child를 메서드 후보에서 제외하고, 메서드 후보가 없을 때만
기존 fallback을 사용하도록 수정했다. Nest 전용 예외가 아니라 모든 SCIP framework
adapter가 같은 resolver를 사용한다.

### 공통 문제 B — 멀티라인 annotation/decorator 아래 handler 누락

NestJS의 `@ApiOkResponse({ type: ... })`가 route decorator와 메서드 사이에 있으면
route만 찾고 handler가 미확정으로 남았다. HTTP decorator를 만날 때까지 최대 32줄을
확인하며 빈 줄·주석·멀티라인 decorator를 건너뛰도록 공통 scanner를 수정했다.
최신 Nest 결과에서 auth route 11개가 모두 `AuthController` 메서드로 연결됐다.

### 공통 문제 C — 테스트 route와 문자열 URL 오탐

Ghost에서 Express route 417개 중 테스트 fixture가 운영 route와 섞였고,
`config.get('url')` 뒤의 문자열 URL이 route path가 되어 `/foo"` 같은 값도 나왔다.
원인은 JS parser가 호출의 첫 번째 argument가 아니라 한 줄 전체의 첫 `/...` 문자열을
선택한 것이었다.

JS/TS registration-routing은 이제 실제 HTTP method 호출의 첫 argument만 path로
읽고, 문자열·주석 마스킹 위치를 이용해 문자열 내부의 `app.get(...)`을 호출로
인식하지 않는다. 또한 모든 framework fact에 테스트 경로를 `source_scope=test`,
`isTest=true`로 기록한다. 테스트를 삭제하지 않고 운영 route와 분리하는 방식이다.
compat inventory도 문자열 형태의 `isTest`를 boolean으로 복원한다.

Ghost 재분석 결과는 Express JavaScript route 417→354, test route 46개 분리,
따옴표로 끝나는 path 0개였다.

### 공통 문제 D — Ruby Rack pack이 Rails route 전체를 중복 생성

Discourse에서 Rails와 Rack이 동일한 `config/routes.rb`와 request spec을 모두
스캔해 각각 9,194개 route를 만들었다. Rack은 `config.ru`에 존재한다는 이유만으로
Ruby 전체의 `get`/`post` DSL을 자기 route처럼 주장했다.

framework detection(팩이 존재함)과 route ownership(어떤 파일의 route를 팩이
소유하는가)이 분리되지 않은 것이 근본 원인이다. Rack route owner는 이제
`Rack::Builder`, 실제 `run <app>`, path를 가진 `map '/prefix'` DSL이 있는 파일로
제한한다. `map = ...`, `def run`, 주석은 Rack registration이 아니다.

Discourse 재분석에서 Rails 9,194 route는 유지되고 Rack route는
9,194→27→0으로 감소했다. 마지막 0은 일반 Ruby 파일의 route DSL을 Rack이
소유하지 않도록 한 결과이며, Rack middleware facts는 별도로 남는다.

### 공통 문제 E — C/C++ shared header가 catalog 순서에 따라 숨겨짐

`.h` 파일이 C provider에 먼저 배정되면 C++ Drogon 선언을 framework discovery가
보지 못해 Drogon 팩이 감지되지 않았다. provider ownership은 단일 언어로 유지하되
framework source index에서는 `.h`/`.inc`를 일치하는 C와 C++ catalog 모두에
노출하도록 수정했다.

### 공통 문제 F — 의존성/빌드 컨텍스트가 없는 clone의 semantic 오판

Bagisto는 `vendor/autoload.php`와 `vendor/composer/installed.php`가 없는 clean
clone에서 scip-php가 없는 `database/factories/`를 스캔하다 실패했다. Composer
metadata가 없으면 PHP semantic을 `excluded`로 표시하고 Laravel structural facts
468개는 유지하도록 preflight를 추가했다.

Serverpod은 `.dart_tool/package_config.json`이 없어 Dart semantic을 억지로 확정하지
않고 `excluded` diagnostic과 structural Serverpod facts 26개를 남긴다. 주석의
`mail service`가 SERVICE fact가 되던 오탐은 주석 제거로 19→13 RPC endpoint가 됐다.

Vaultwarden은 build script의 `make`, Diesel backend, Windows OpenSSL 개발 의존성이
없어 Rust provider가 partial 결과를 반환했다. `indexed-partial`과 diagnostic을
유지하며, Axum이 `.route()` 하나로 감지되지 않도록 Axum/Rocket pack에 package
identity를 추가했다. 임시 `make` shim은 field lab에서만 사용했다.

### 언어별 현장 결과

- **NestJS**: TypeScript 182개 파일 색인, auth route 11개가 모두 controller
  method로 확정.
- **Drogon/C++**: CMake는 `jwt-cpp`, `Bcrypt`, Drogon package 부재로 실패했다.
  수동 compile DB 후 C 10/10, C++ 19/20 색인, Drogon route 10개 중 4개 확정,
  6개 unresolved. 외부 header 부재를 성공으로 포장하지 않았다.
- **gvmd/C**: C 348개가 usable compile context 부재로 `excluded`, 기존 C pack에
  GMP command/event 흐름이 없어 structural fact 0. C의 HTTP route만 확장하는
  것으로 해결되지 않는 제품 범위 gap이다.
- **Ghost**: `ghost/core`에서 JavaScript 2,126/2,126, TypeScript 333/333
  색인. 운영/테스트 scope 및 Express URL 오탐이 수정됐다.
- **Discourse**: Ruby 10,795/10,795 색인. 그러나 Windows `x64-mingw-ucrt`가
  Gemfile.lock 플랫폼 목록에 없어 Ruby LSP composed bundle setup이 반복 실패했고,
  gem resolution 완전성은 보장하지 않는다. JavaScript 2,818개 중 1,981개,
  TypeScript 58개 중 57개만 색인되어 `indexer-failed`로 표시됐다. 여러 plugin이
  존재하지 않는 `discourse/tsconfig-plugin`을 참조한 것이 원인이다.

### 판정 계약

| 상태 | 의미 |
|---|---|
| 확정 | provider symbol 또는 명확한 registration/annotation 근거가 있음 |
| 후보/미확정 | 구조는 보이나 대상 symbol·runtime 경로·외부 metadata가 부족함 |
| test-only | 테스트에서만 발견됨. 제거하지 않고 별도 속성으로 보존 |
| excluded/indexer-failed | semantic 결과가 신뢰 불가능해 확정 관계에 사용하지 않음 |

“팩 감지”는 “모든 route가 그 팩 소유”와 다르고, “파일 색인”은 “외부 dependency와
runtime dispatch까지 해석”과 다르다. 이 구분을 깨는 수정은 회귀 테스트 없이
허용하지 않는다.

### 변경 파일과 검증

- `code_memory/rust/src/frameworks.rs`: shared header discovery, test scope metadata,
  JS first-argument route parsing, Rack route ownership
- `code_memory/rust/src/frameworks/facts.rs`: indexed method/parameter filtering,
  Nest multiline decorator, Drogon macro method parsing
- `code_memory/rust/src/frameworks/tests.rs`: 위 동작 회귀 테스트
- `code_memory/rust/src/providers/analysis.rs`: Composer metadata fail-closed preflight
- `code_memory/rust/src/compat.rs`: `isTest` 문자열 복원
- `code_memory/packs/framework/rust/{axum,rocket}/pack.json`: package identity activation

검증은 `cargo fmt --all`, `cargo test --locked --quiet` 153 passed,
`cargo build --release --locked --quiet`를 통과했다.
framework provider gate 84/84, cross-language flow gate 12/12, uniform core quality
gate 12/12도 통과했다. provider gate에서 발견된 Blazor와 .NET MAUI template
event 회귀는 comment-free template evidence로 보정한 뒤 다시 84/84를 확인했다.

### 남은 근본 gap

1. route별 `declared`/`test-only`/`active-from-main`/`unknown` 상태 모델과 main
   entrypoint reachability가 아직 완성되지 않았다.
2. HTTP 외 C GMP command/event, Go gRPC dispatch, sqlc generated query→원본 SQL→
   table mapping을 공통 boundary 모델로 확장해야 한다.
3. Ruby LSP platform mismatch와 TypeScript plugin project-reference 실패를 provider가
   반복 실행하지 않고 사전 진단해 부분 결과와 원인을 빠르게 반환해야 한다.
4. 대형 monorepo는 전체 root와 focused runtime scope를 구분하고 분석 시간·메모리
   한계를 UI에 표시해야 한다. Discourse 전체 Ruby 분석은 약 1GB에 접근했다.

특정 레포 이름이나 경로를 하드코딩해 숫자를 맞추지 않는다. 다음 변경도 gap을
회귀 fixture로 먼저 고정한 뒤 공통 엔진에 최소 패치로 반영한다.

## 127. 2026-08-01 — cross-language field-test hardening 회귀 감사

### 확인한 반대 방향 회귀

기존 Discourse Rack 중복 제거 규칙과 JavaScript 문자열 URL 제거 규칙이 정상
코드까지 지우지 않는지 반대 fixture를 추가했다.

- Rack 전용 `config.ru`의 `map '/users'`는 `HTTP_ROUTE`로 계속 보존된다.
- Rails `config/routes.rb`, `map = ...`, `def run`은 Rack route 소유로 잘못 분류되지 않는다.
- 정상 TypeScript `router.get("/users/:id", handler)`는 정확히 한 route로 보존된다.
- 문자열 안의 `router.get('/string-only', handler)`는 route로 생성되지 않는다.

### 결과 모델과 UI 상태 계약

architecture 언어 요약에 선택적 `exclusion_reason`과 `exclusion_scope`를
추가했다. 현재 근거가 확인되는 언어 제외 사유는 다음의 안정적인 값으로
노출한다.

| reason | 의미 |
|---|---|
| `missing-dependency` | Composer/Dart package 등 의존성 메타데이터 없음 |
| `missing-compile-context` | C/C++ compile DB, SDK, build tool 등 컴파일 문맥 없음 |
| `unsupported-framework` | 프레임워크 adapter가 지원하지 않음 |
| `runtime-reachability-unknown` | 정적 코드만으로 실행 경로 확인 불가 |

UI는 `excluded`를 단순 경고로만 표시하지 않고 범위와 사유 문구를 함께 보여준다.
`test-only`는 언어 전체 제외 사유가 아니라 route/fact의 `source_scope`와
`isTest` 상태이므로 기존 fact 단위 속성으로 유지했다. `runtime-reachability-unknown`
도 언어 제외에 억지로 붙이지 않고 실제 route reachability 모델이 생길 때만
사용한다. route fact의 runtime 상태는 `not-assessed`로 시작하며, runtime 분석을
시도했지만 결론을 못 낸 경우에만 `unknown`을 사용한다.

### 테스트 러너에서 발견한 별도 근본 문제

framework/flow PowerShell gate가 provider의 정상적인 stderr 진단을
`$ErrorActionPreference = 'Stop'`으로 실패 처리했다. bridge의 exit code와 출력
JSON을 기준으로 판정하도록 호출 구간의 오류 정책을 분리했다.

또한 flow fixture의 `go.mod`가 PowerShell `Set-Content`의 UTF-8 BOM 때문에
`gopls`에서 `unexpected input character '\ufeff'`로 실패했다. 공통 fixture writer를
UTF-8 no-BOM으로 바꿔 Go 호출 관계가 다시 생성되도록 했다.

### 변경 및 검증

- `code_memory/rust/src/frameworks/tests.rs`: Rack/JS 정상 보존 반대 fixture
- `code_memory/rust/src/architecture.rs`, `architecture/model.rs`: 언어 제외 사유
- `code_memory/rust/src/cache.rs`: architecture cache v20
- `src/types/workspace.ts`, `CodeSourceSection.tsx`, `forms.css`: 사유 전달·표시
- `code_memory/tests/gates/run-framework-{provider,flow}-gate.ps1`: stderr 및 no-BOM fixture 처리

검증 결과:

- Rust unit tests: 155 passed
- Frontend typecheck: passed
- Frontend Vitest: 28 files, 155 tests passed
- Release build: passed
- Framework provider gate: 84/84
- Cross-language flow gate: 12/12
- Uniform core quality gate: 12/12

이번 패치는 특정 레포의 route 숫자를 맞춘 것이 아니라, 정상 코드를 삭제하지
않는 반대 회귀와 실패 원인 표시를 공통 계약으로 고정한 것이다.

## 128. 2026-08-01 — 깨끗한 CI checkout에서 provider gate가 실패한 원인

main push 후 GitHub Actions `Verify uniform language core quality`가
`typescript: status is not indexed (missing-tool)`로 실패했다. 원인은 엔진이
아니라 CI가 `code_memory/providers`를 준비하지 않은 상태에서 provider 의존
게이트를 실행한 것이다. 해당 디렉터리는 gitignore 대상인 로컬/릴리즈 provider
자산이라 깨끗한 checkout에는 존재하지 않는다.

해결은 없는 provider를 CI에 있는 것처럼 검사하는 것이 아니다. 기본 CI에서는
provider 자산과 무관한 Rust/프론트/bridge 계약 검증만 실행하고, provider gate와
cross-language field gate는 provider bundle이 준비된 로컬·릴리즈 검증 단계에서
실행하도록 workflow 경계를 분리했다. 이로써 `missing-tool`을 실제 분석 실패로
오인하지 않고, provider가 없는 환경에서도 실패 상태를 정직하게 유지한다.

## 129. 2026-08-01 — v0.1.0-preview.7 배포 검증

현재 main `15c2afb`를 기준으로 Windows x64 내부 prerelease
`v0.1.0-preview.7`을 배포했다.

- main CI `30690959067`: 성공
- installer: `Backend.Visual.Map_0.1.0_x64-setup.exe`
- installer SHA-256: `B76C1EC27443886181319F0885AD37E0B30CB52BCDEB8EE21278EB71FC40A015`
- GitHub asset digest와 로컬 SHA-256 일치
- 버전·PE 형식·engine pin·third-party notices/license non-install smoke 통과

설치 smoke는 개발 PC에 설치하지 말라는 운영 조건에 따라 중단했다. 중단 시점에
설치 레지스트리 항목, 실행 중인 앱/installer 프로세스, 임시 smoke 디렉터리를
확인했고 모두 정리했다. 따라서 이번 프리릴리즈는 설치 smoke를 포함하지 않은
내부 검증용 prerelease이며, 별도 Windows 격리 runner에서 설치·실행·uninstall
smoke를 수행한 뒤 배포 신뢰도를 더 높일 수 있다.

## 130. 2026-08-02 — bracket 혼합 프론트/백엔드 route surface 및 focused flow hardening

### 재현한 문제

`evroon/bracket`처럼 React/Vite 화면과 FastAPI 백엔드가 한 저장소에 있는 경우,
중첩 `docs/package.json`의 Next.js 의존성만으로 Next pack이 저장소 전체의
`pages/` 파일을 route로 수집했다. 그 결과 React Router의 `/login`, `/` 같은
화면 이동 경로가 백엔드 API 목록에 섞이고 API 개수가 부풀었다.

또한 route 이름에 이미 `POST /token`이 들어 있는데 UI가 method를 다시 앞에
붙여 `POST POST /token`을 표시했고, focused flow가 주 경로 외의 모든 간선을
동시에 그려 대형 fan-out에서 선 교차·노드 겹침·초기 수평 잘림이 발생했다.

### 근본 수정

- framework fact마다 `routeSurface`를 기록한다.
  - `backend-api`: 실제 서버 HTTP endpoint
  - `ui-navigation`: React Router/Next pages/Nuxt pages/SvelteKit +page 등 화면 경로
- `CodeInventory`와 snapshot은 surface를 유지하고, API 탭·API flow·검색·Atlas
  API band에는 `backend-api`만 넣는다.
- 프로젝트 탐색기에는 `코드`와 `화면 라우트`를 별도 섹션으로 표시한다.
- React Router `<Route path="...">`는 UI route로만 수집하며 method는 `ANY`로
  고정해 서버 API처럼 확정하지 않는다.
- Next/모노레포 metadata 신호는 실제 `package.json` dependency key를 정확히
  확인하고, 일치한 metadata 디렉터리 범위 안의 source만 filesystem route 후보로
  사용한다. `docs`가 `frontend`를 오염시키지 않는다.
- 기존 snapshot이 이전의 API-only route 의미를 계속 사용하지 않도록 code adapter
  version을 5에서 6으로 올려 재색인을 유도한다.
- route 표시에는 공통 `routeDisplayName`을 사용해 method 중복을 제거한다.
- focused flow는 주 경로와 가까운 보조 관계 6개만 기본 캔버스에 배치하고, 나머지
  관계는 `보조 관계 N개` 접기 목록에서 근거를 유지한다. 같은 y row의 노드는 최소
  간격을 보장하고 route 선택이 바뀌면 캔버스를 왼쪽 위로 되돌린다.

### 회귀 검증

- nested Next package가 sibling `frontend/pages`를 claim하지 않는 fixture
- React Router 단일/멀티라인 `<Route>`가 `ui-navigation`으로만 생성되는 fixture
- Next pages/app/api, Nuxt pages/server, SvelteKit +page/+server, 일반 FastAPI의
  surface 분류 fixture
- API catalog가 UI route를 제외하고 코드 탐색기의 `화면 라우트`에만 넣는 frontend test
- focused graph 보조 간선 6개 제한과 접힘 근거 보존 frontend test
- Rust framework tests 61개 통과
- 전체 frontend Vitest 28개 파일·157개 테스트 통과, typecheck 및 production build 통과
- 전체 `code_memory` Rust 테스트 159개 통과, `src-tauri` 테스트 251개 통과·5개 ignored

### 남은 명확한 경계

이번 변경은 route surface 오염과 focused visualization 신뢰성을 해결했다.
`performLogin → axios.post("token") → POST /token`처럼 클라이언트 HTTP 호출을
백엔드 endpoint와 확정 연결하는 것은 별도의 `CLIENT_REQUEST` 공통 IR과 언어별
literal/기본 URL/라우터 mount 해석이 필요하다. 현재는 이를 CALLS나 API로 위장하지
않고, 해당 관계가 없으면 확인 안 됨으로 남기는 것이 정확하다. 다음 단계에서
JavaScript/TypeScript만 임시 패치하지 않고 모든 지원 언어에 같은 확정·후보·unknown
계약을 적용해야 한다.

## 131. 2026-08-02 — 다언어 CLIENT_REQUEST 공통 계약과 focused API 연결

### 이번에 해결한 근본 문제

기존 CALLS는 함수 내부 호출만 표현할 수 있어 frontend performLogin →
axios/fetch/requests → backend POST /token을 서버 내부 호출처럼 연결할 공통
관계가 없었다. 또한 4줄 겹침 윈도우로 소스 패턴을 찾을 때 호출 위치를 윈도우
시작 줄로 기록하면 같은 호출이 여러 번 생성될 수 있었다.

### 적용한 공통 설계

- ClientRequest IR을 추가했다. method/path/source/line/caller/stable ID와
  resolution/evidence를 모든 언어에서 동일하게 보존한다.
- 정적 literal·안전한 local constant·기본 URL만 static-confirmed로 승격한다.
  generic receiver, 상대 URL, 동적 URL은 candidate 또는 unknown으로 남긴다.
- 테스트 전용 경로는 excluded와 excluded:test-only 근거로 분리하고 서버 API
  관계를 만들지 않는다. 주석·문자열 속 가짜 호출도 문자열 마스킹 후 검색한다.
- fetch, Axios, Python requests/httpx, Java RestTemplate, Go net/http,
  Rust reqwest, PHP Laravel Http, Ruby Faraday, C/C++ cpr/libcurl, Dart
  http, C# GetAsync 등 12개 active 언어 확장자에 같은 상태 계약을 적용했다.
- method/path가 서버 route와 유일하게 맞을 때만 snapshot에 CLIENT_REQUEST
  관계를 만든다. :id, {id}, <id> 동적 route는 segment 수와 고정 segment를
  확인하고, 동점 route는 후보로 둔다.
- API focused flow에는 Client Request → Backend API → Handler → ...를 별도
  incoming lane으로 추가했다. 후보는 점선/후보 상태, 확정은 확정 상태로 유지해
  클라이언트 요청을 서버 내부 CALLS와 섞지 않는다.
- 분석 중 프로젝트 코드를 실행하지 않으며, 저장하는 URL은 query/hash를 제거한
  path 또는 <unresolved>만 사용한다. header/body/token/row data는 저장하지 않는다.
- 요청 분석 capability가 캐시된 기존 결과를 재사용하지 않도록 code adapter
  version을 6에서 7로 올려 재색인을 유도했다.

### 발견 후 수정한 회귀

- 겹친 4줄 윈도우가 newline을 공백으로 합쳐 실제 호출 줄을 잃던 중복 생성.
- Go NewRequest/NewRequestWithContext가 method 인자를 REQUEST로 버리던 문제.
- C# .GetAsync marker가 앞 receiver의 마지막 알파벳 때문에 항상 제외되던 문제.
- 문자열 내부의 단일/멀티라인 fake request가 실제 호출로 인식될 수 있던 문제.
- fetch("/path")의 표준 기본 method인 GET을 누락해 불필요한 후보로 남기던 문제.
- 동적 서버 route의 임의 문자열 비교를 막고 segment 수·고정 구간 기준으로만 매칭.

### 검증

- src-tauri: 257 passed, 5 ignored
- client_requests conformance: 3 tests passed, 12 active language extensions
- atlas: client request → backend route snapshot 및 focused incoming lane 회귀 통과
- frontend Vitest: 28 files, 157 tests passed
- npm run typecheck: passed
- npm run build: passed (기존 단일 JS chunk 500KB 초과 warning만 유지)
- cargo fmt --check, git diff --check: passed

### 아직 정직하게 남은 경계

1. 환경변수·DI·reflection·generated client·runtime base URL은 기본 정적 분석만으로
   확정하지 않는다. 현재는 candidate/unknown이며, 실행되지 않은 경로도 부재로
   판정하지 않는다.
2. 현재 focused API 화면은 선택 route에 연결된 최대 4개 client request를 표시한다.
   더 많은 호출은 후속 접기/근거 목록 정책이 필요하다.
3. provider AST가 직접 제공하는 언어별 request fact가 아직 없어, 현재 공통 추출은
   bounded source scanner다. 대규모 저장소에서 한도에 도달하면 capability gap을
   보여주고 provider AST fact로 교체하는 것이 다음 단계다.
4. runtime observation은 기본 기능으로 추가하지 않았다. 도입할 때도 opt-in 격리
   runner·redaction·trace ID만 저장하는 별도 계약과 release gate가 필요하다.

## 132. 2026-08-02 — 대형 모노레포 `get_architecture` JSON 수신 경계

### 증상

`makeplane/plane` 분석 중 코드 인덱싱 준비 창에
`새 코드 인덱스를 검증하지 못했습니다: 코드 엔진 get_architecture 응답이 올바른 JSON이 아닙니다`
가 표시됐다. 이 메시지는 UI 렌더링 문제가 아니라 Tauri가 코드 엔진 stdout을
JSON으로 해석하지 못해 새 인덱스를 폐기한 상태를 뜻한다.

### 원인 분석

동일한 Plane 저장소를 현재 번들 엔진으로 직접 분석하면 인덱싱은 종료 코드 0으로
완료되고, 9,888개 architecture node·12,788개 edge·61개 flow·5,165개 진단을
포함한 약 12.8MB의 유효한 `code-memory.architecture-index.v3` JSON이 생성된다.
따라서 Plane 자체를 “분석 불가”로 처리하는 것은 틀리다.

직접 확인한 실제 원인은 앱 공통 stdout 보안 마스킹이었다. 기존 `redact_secrets`는
JSON을 구조로 읽지 않고 모든 문자열을 `password`/`token` 패턴으로 훑었다. Plane의
정상적인 `forgot-password:module` 같은 코드 경로를 key-value로 오인해 JSON 내부를
변형했고, 그 결과 엔진이 만든 유효한 JSON이 앱에 도착할 때는 파싱 불가능한 문자열이
됐다. 여기에 구버전/환경별 sidecar의 BOM·pretty JSON·앞뒤 로그 framing도 기존
수신기가 취약하게 처리하는 보조 경계였다.

### 패치

- 순수 JSON stdout은 먼저 `serde_json::Value`로 구조 파싱한 뒤 민감한 key의 value만
  `[REDACTED]`로 바꿔 JSON 형태를 보존한다.
- JSON string 안의 일반 텍스트도 별도로 마스킹하므로 보안 처리는 유지한다.
- `engine_json_value`는 BOM, pretty JSON, JSON 앞뒤 로그 framing을 허용하되 완전한
  JSON value만 추출하고, 찾지 못하면 계속 fail-closed한다.
- Plane의 secret-like code path를 포함한 redaction 회귀와 JSON framing 회귀 테스트를
  추가했다.

### 검증

- `cargo test --manifest-path src-tauri/Cargo.toml engine_json_value`: 3 passed
- 전체 Tauri lib test: 260 passed, 5 ignored
- `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings`: passed
- 실제 Plane architecture 직접 생성 및 JSON parse: passed

현재 공개된 `v0.1.0-preview.10` 설치 파일에는 이 수신기 패치가 아직 포함되지
않았다. 새 내부 프리뷰를 만들기 전까지는 해당 릴리즈에서 같은 현상이 재현될 수 있다.

## 133. 2026-08-02 — Plane 클라이언트 URL 분석 중 작업 스레드 종료

### 증상

`makeplane/plane` 분석 중 준비 창에 `코드 분석 작업이 비정상 종료되었습니다`가
표시됐다. 개발자 도구에는 별도로 `tauri`·`react` 중복 key 경고와
`dialog.confirm not allowed` 오류가 함께 나타났다.

### 원인 분석

세 문제는 하나의 원인이 아니었다.

- Plane의 정적 클라이언트 요청에 `http://...` 절대 URL이 포함되어 있었고,
  공통 URL 정규화기가 `http://`도 8바이트 접두사처럼 처리했다. 실제 접두사는
  7바이트이므로 `value[8..]`가 분석 스레드에서 panic을 일으켰고, 부모는 join
  실패를 일반적인 비정상 종료 메시지로만 표시했다.
- 코드 품질 목록은 언어와 프레임워크를 같은 형제 목록으로 렌더링하면서 각각
  `id`만 key로 사용했다. `tauri`, `react`처럼 두 목록에 모두 존재하는 ID가
  React key 충돌을 일으켰다.
- 삭제 확인은 Tauri dialog plugin 호출 권한과 맞지 않는 경로에서 실행됐다.
  capability에는 `dialog:allow-confirm`을 명시하고, Tauri 밖에서는 브라우저
  `window.confirm`으로 폴백해야 한다.

### 패치

- `http://`와 `https://`를 각각 정확한 접두사 길이로 처리하도록
  `normalize_url_path`를 수정하고 두 scheme의 회귀 테스트를 추가했다.
- 문자열 마스킹 결과 변환도 `from_utf8` panic 대신 lossless replacement를
  사용해 분석 스레드의 불필요한 panic 경로를 제거했다.
- 언어 key는 `language:<id>`, 프레임워크 key는 `framework:<id>`로 namespace를
  분리했다.
- 삭제 확인을 Tauri dialog `confirm`과 브라우저 폴백을 공유하는 helper로
  통일하고 `dialog:allow-confirm` 권한을 추가했다. Tauri 명령을 사용할 수
  없는 개발/비정상 실행 환경에서는 브라우저 확인창으로 안전하게 폴백한다.

### 검증

- Plane field harness는 기존 URL panic을 더 이상 발생시키지 않고, 해당
  FastAPI 전용 fixture assertion까지 진행했다. fixture 자체가 Plane 구조와
  일치하지 않아 최종 assertion은 의도대로 실패했으며 분석 스레드 panic과는
  구분된다.
- Tauri lib test: 265 passed, 5 ignored
- `cargo fmt --check`, `cargo clippy --lib -- -D warnings`: passed
- frontend typecheck/lint/test: passed (36 files, 189 tests)
- coverage: statements 58.18%, branches 46.58%, functions 60.20%, lines 58.74%
- frontend build, dead-code check, dependency audit 및 local security audit: passed

### 2026-08-02 신뢰성·경계 패치

- 엔진 프로세스 출력 상한 초과 시 스트림과 프로세스 트리를 즉시 종료하고,
  비정상/다중 JSON 응답을 성공 결과로 오인하지 않도록 엄격히 거부했다.
- 작업공간과 provider bundle 추출에 OS 파일 잠금을 추가해 다중 앱 인스턴스의
  동시 쓰기·동시 압축 해제를 직렬화하고, bundle manifest/checksum 위변조를
  감지하면 캐시를 재생성하도록 했다.
- DB/소스 스캔의 제한·생략을 부분 상태와 gap으로 기록해 표시된 개수와 실제
  전체 개수를 혼동하지 않도록 했다. 비동기 인덱싱 결과는 최신 작업만 반영한다.
- IPC 응답의 중복 ID, 잘못된 수치, 끊어진 edge, 잘못된 workspace/profile 참조를
  런타임 계약에서 fail-closed로 거부한다.
- React 목록/그래프의 중복 key, 반복 HTTP method 표기, 프론트 SPA route의 API
  오분류를 수정했다.

### 검증

- Rust lib test: 265 passed, 5 ignored; clippy warnings: 0
- frontend: 36 files, 189 tests passed; typecheck/lint/format/deadcode/build passed
- local security audit 및 npm dependency audit: passed
