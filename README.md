# VisualMap

로컬 코드베이스를 분석해 **비즈니스 도메인 → 기능 → 실행 흐름**의 시각 지도로 보여주는 개발자 도구입니다.

VisualMap은 정적으로 확인할 수 있는 사실과 추정이 필요한 의미를 구분합니다. 호출 관계, 진입점, 리소스, 실행 흐름에는 근거와 해석 상태를 함께 보존하고, Codex는 코드 구조를 사람이 이해하기 쉬운 이름과 한 줄 설명으로 바꾸는 데 사용합니다.

## 핵심 개념

| 단계 | 의미 |
| --- | --- |
| **도메인** | 파일·심볼·진입점·리소스·관계 신호를 바탕으로 묶은 코드 영역 |
| **기능** | 도메인 안에서 외부에 노출되거나 의미 있는 동작 단위 |
| **실행 흐름** | 진입점에서 호출·분기·예외·동적 경계를 따라가는 흐름 |

정적 분석의 참조 상태는 다음처럼 구분합니다.

| 상태 | 의미 |
| --- | --- |
| `confirmed` | 소스에서 대상을 유일하게 특정함 |
| `candidate` | 가능한 대상이 여러 개라 확정하지 못함 |
| `unknown` | 저장소 안에서 대상을 찾지 못함 |
| `dynamic` | 리플렉션·동적 로딩·eval 등 실행 시점에 결정됨 |

분석에 실패한 파일이나 부분 파싱도 조용히 버리지 않습니다. 결과의 `status`와 `diagnostics`에 파일 경로와 원인을 남깁니다.

## 현재 구성

```text
./                              Rust 정적 분석 엔진
  src/project/                    파일 스캔과 프로젝트 메타데이터
  src/languages/                  언어별 AST 분석과 공통 사실 변환
  src/frameworks/                 프레임워크 감지, 라우트, 리소스 보강
  src/facts/                      참조·선언·제어 흐름 사실
  src/flow/                       실행 흐름 그래프
  src/domain/                     도메인 그룹화
  src/postprocess/                Codex 입력용 컨텍스트 축약
  src/semantic/                   선택적 Codex 의미 분석
  src/views/                      프론트엔드용 Overview 투영
  src/pipeline/                   단계 실행과 프로파일링
  config/                         분석 정책과 언어 규칙
  tests/                          단위·통합·품질 게이트

frontend/                         Tauri + React 데스크톱 앱
  src/                            캔버스와 분석 진행 UI
  src-tauri/                      분석 엔진·Codex CLI 연동 및 로컬 저장소
```

## 지원 언어

JavaScript, TypeScript, Python, Java, C, C++, C#, Go, Rust, Dart를 지원합니다.

언어별 AST를 공통 사실 모델로 변환한 뒤, 프레임워크별 어댑터가 HTTP 라우트·RPC 등록·콜백·ORM 모델·외부 리소스 신호를 보강합니다.

## 분석 흐름

```text
프로젝트 스캔
  → 언어별 분석
  → 참조 해석·정적 그래프
  → 프레임워크·리소스 보강
  → 실행 흐름 그래프
  → 도메인·기능 그룹화
  → Codex 입력 전처리
  → 선택적 Codex 의미 분석
  → 프론트엔드 Overview
```

## 엔진 실행

```bash
cargo run --release -- <프로젝트-경로>
```

주요 옵션:

| 옵션 | 설명 |
| --- | --- |
| `--output=<경로>` | 결과 JSON 저장 경로 |
| `--compact` | JSON을 한 줄로 출력 |
| `--profile` | 단계별 시간과 산출량 출력 |
| `--no-cache` | 캐시를 사용하지 않고 다시 분석 |
| `--config=<경로>` | 분석 설정 파일 지정 |

Codex 입력 컨텍스트는 정적 결과에서 별도로 생성할 수 있습니다.

```bash
cargo run --release -- postprocess codex-context \
  --input=<static-result.json> \
  --output=<codex-context.json> \
  --config=config/analysis.default.toml \
  --pretty
```

Codex 의미 분석까지 실행하려면:

```bash
cargo run --release -- semantic review \
  --input=<codex-context.json> \
  --output=<semantic-result.json> \
  --project-root=<프로젝트-경로> \
  --config=config/analysis.default.toml \
  --model=<codex-model> \
  --profile
```

Codex 호출은 `--ephemeral`, read-only sandbox로 실행되며 사용자의 Codex 세션을 이어받거나 대화 기록을 남기지 않습니다.

## 데스크톱 앱 실행

```bash
cd frontend
npm install
npm run tauri dev
```

개발 모드의 분석 결과는 `tests/dev` 아래 워크스페이스에 저장됩니다. 릴리스 모드에서는 운영체제의 사용자 데이터 폴더 아래 `VisualMap`을 사용합니다.

## 설정

기본 설정은 `config/analysis.default.toml`에 있습니다.

- `[scan]`: 파일 탐색과 파일 크기 정책
- `[limits]`: 유닛·참조·흐름·출력 상한
- `[languages]`: 확장자와 언어 매핑
- `[paths]`: 제외 경로와 테스트 파일 판별
- `[domains]`: 도메인 신호와 그룹화 가중치
- `[parser]`: 라우트·리소스·동적 호출 규칙
- `[frameworks]`: 매니페스트와 프레임워크 감지 정책
- `[semantic]`: Codex CLI·모델·입력 크기·재시도 정책
- `[postprocess]`: Codex 컨텍스트 축약과 바이트 예산

분석 규칙은 특정 저장소의 파일명이나 심볼에 의존하지 않고 설정과 언어·프레임워크 사실을 기준으로 동작하는 것을 원칙으로 합니다.

## 검증

```bash
cargo test --release -- --include-ignored --nocapture
cargo clippy --release --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 개발 원칙

- 정적으로 확인한 사실과 AI가 해석한 의미를 서로 덮어쓰지 않습니다.
- 확인할 수 없는 관계는 `candidate`, `unknown`, `dynamic`으로 명시합니다.
- 오류를 삼키지 않고 결과 상태와 진단 정보로 노출합니다.
- 테스트 하나에 맞춘 특수 분기를 추가하지 않고, 여러 언어·프레임워크·프로젝트 규모에 같은 규칙을 적용합니다.
- 기능 변경은 검증 가능한 작은 단위로 나누고, 성능과 정확도를 함께 측정합니다.
