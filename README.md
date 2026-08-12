# Visual Map

로컬 코드베이스를 정적 분석해 **도메인 → 기능 → 실행 흐름** 세 단계의 시각 지도로 변환하는 도구.

모든 관계에는 근거와 확신도가 붙는다. 확신할 수 없는 연결은 확신할 수 없다고 표시하며, 추측을 사실처럼 그리지 않는다.

---

## 핵심 개념

### 도메인 → 기능 → 실행 흐름

| 층위 | 의미 |
| --- | --- |
| **도메인** | 이름·경로·진입점·자원 신호로 묶인 코드 영역 |
| **기능** | 도메인 안에서 외부에 노출되는 동작 단위 |
| **실행 흐름** | 함수 하나가 실제로 밟아 나가는 호출·분기·예외 경로 |

### 참조 근거 등급

정적 분석으로 확정할 수 있는 범위는 언어마다 다르다. 그 차이를 숨기지 않고 네 등급으로 나눠 보고한다.

| 등급 | 의미 |
| --- | --- |
| `confirmed` | 호출 대상을 소스에서 유일하게 특정함 |
| `candidate` | 대상 후보가 여럿이며 정적으로 좁힐 수 없음 |
| `unknown` | 대상을 저장소 안에서 찾지 못함 |
| `dynamic` | 리플렉션·`eval`·동적 로딩 등 실행 시점에만 결정됨 |

### 실패를 숨기지 않는다

파일을 읽지 못하거나 파싱에 실패해도 조용히 건너뛰지 않는다. 결과의 `status`가 `partial`로 내려가고, `diagnostics`에 파일 경로와 원인이 함께 남는다.

```
PARSE_ERROR                  문법 오류가 있는 AST를 부분 분석함
PARSE_FAILED                 파싱 자체가 실패함
SOURCE_READ_FAILED           소스를 읽지 못함 (예: 유효하지 않은 UTF-8)
FILE_READ_FAILED             파일 접근 실패
FILE_TOO_LARGE               크기 상한 초과
DIRECTORY_READ_FAILED        디렉터리 순회 실패
DIRECTORY_ENTRY_UNAVAILABLE  디렉터리 항목 접근 실패
FILE_TYPE_UNAVAILABLE        파일 종류 판별 실패
LANGUAGE_SETUP_FAILED        언어 파서 초기화 실패
PARSER_RULE_INVALID          설정된 파서 규칙이 잘못됨
CODEX_*                      의미 분석 단계 실패 (선택 기능)
```

---

## 지원 언어

tree-sitter로 파싱해 언어별 문법을 공통 구조로 정규화한다.

JavaScript · TypeScript · Python · Java · C · C++ · C# · Go · Rust · Dart

웹 프레임워크의 라우트, RPC 등록, 콜백 등록, ORM 모델은 언어와 무관하게 공통 진입점·자원 사실로 변환된다.

---

## 저장소 구조

```
backend/code_analysis_engine/    Rust 정적 분석 엔진
  src/
    project/       파일 스캔과 메타데이터 수집
    languages/     언어별 파싱 → 공통 사실 변환
    frameworks/    프레임워크 감지와 진입점 추출
    facts/         참조 해석과 근거 등급 판정
    flow/          실행 흐름 그래프 구성
    domain/        도메인 그룹화
    semantic/      선택적 의미 분석 (Codex)
    views/         Overview 투영
    pipeline/      단계 실행과 프로파일링
  config/          기본 설정
  tests/           통합 테스트

frontend/                        데스크톱 UI (준비 중)
```

---

## 실행

```bash
cargo run --release -- <프로젝트-경로>
```

| 옵션 | 설명 |
| --- | --- |
| `--compact` | 들여쓰기 없는 JSON으로 출력 |
| `--output=<경로>` | 표준 출력 대신 파일로 저장 |
| `--no-output` | 결과 JSON을 출력하지 않음 (측정 전용) |
| `--profile` | 단계별 소요 시간과 산출량을 stderr에 출력 |
| `--config=<경로>` | 기본 설정을 부분 override |
| `--codex` | 의미 분석 단계 활성화 |

> 결과 JSON은 대상 저장소 규모에 따라 수백 MB에 이른다. 큰 저장소에서는 `--output` 또는 `--no-output`을 함께 쓴다.

예시:

```bash
cargo run --release -- "D:\my-project" --compact --output=result.json
cargo run --release -- "D:\my-project" --profile --no-output
```

---

## 출력 구조

```jsonc
{
  "schemaVersion": …,
  "analysisId":    …,
  "status":        "ready" | "partial",
  "project":       { … },        // 대상 저장소 정보
  "files":         [ … ],        // 파일별 메타데이터
  "summary":       { … },        // 파일 수, 바이트, 언어 분포
  "diagnostics":   [ … ],        // 실패·부분 분석 기록
  "elapsedMs":     …,
  "overview": {
    "domains":            [ … ],  // 도메인
    "features":           [ … ],  // 기능
    "units":              [ … ],  // 함수·클래스 등 코드 단위
    "staticGraph":        { … },  // 참조 관계 그래프
    "executionFlows":     { … },  // 실행 흐름
    "entrypoints":        [ … ],  // HTTP·RPC·CLI 진입점
    "resources":          [ … ],  // DB·파일·네트워크·환경변수 접근
    "dynamicBoundaries":  [ … ],  // 정적으로 넘을 수 없는 경계
    "detectedFrameworks": [ … ],
    "coverage":           { … }   // 파싱 성공·부분·실패 집계
  }
}
```

`coverage`는 언어별로 `parsedFiles` / `partialFiles` / `failedFiles`를 나눠 담는다. 분석 결과를 얼마나 신뢰할 수 있는지는 이 값으로 판단한다.

---

## 분석 파이프라인

`--profile`로 각 단계의 소요 시간과 산출량을 확인할 수 있다.

```
project_scan                 파일 수집과 언어 판별
language_analysis            파싱 → 유닛·참조·진입점·자원 추출
reference_resolution         참조 대상 해석과 근거 등급 판정
framework_detection          매니페스트·코드 신호로 프레임워크 감지
framework_fact_enrichment    프레임워크별 사실 보강
static_relation_graph        참조 관계 그래프 구성
execution_flow_graph         함수별 실행 흐름 구성
domain_grouping              신호 기반 도메인 그룹화
codex_semantic_review        의미 분석 (--codex 시에만 실행)
domain_relation_aggregation  도메인 간 관계 집계
overview_projection          최종 Overview 투영
```

---

## 설정

기본값은 `config/analysis.default.toml`에 있으며 실행 파일에 내장된다. `--config`로 필요한 항목만 덮어쓴다.

| 섹션 | 내용 |
| --- | --- |
| `[scan]` | 파일 크기 상한, 숨김 파일 포함 여부, 해시 계산 |
| `[limits]` | 유닛·참조·흐름·출력 크기 상한 |
| `[languages.extensions]` | 확장자 → 언어 매핑 |
| `[paths]` | 제외 디렉터리, 테스트 경로 판별 규칙 |
| `[domains]` | 도메인 그룹화 가중치와 임계값 |
| `[parser]` | 라우트·자원·동적 호출 패턴 |
| `[frameworks]` | 매니페스트 목록과 확신도 |
| `[semantic]` | 의미 분석 설정 |

라우트와 자원 인식 규칙은 설정 파일로 확장할 수 있으며, 새 프레임워크를 추가하는 데 코드 수정이 필요하지 않다.

---

## 테스트

```bash
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

