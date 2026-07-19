# 백엔드 비주얼 맵

백엔드 코드와 관계형 데이터베이스 메타데이터의 관계를 시각화하는 Windows 우선 Tauri + React 데스크톱 앱입니다.

## 빠른 시작

소스에서 실행하려면 다음 명령을 사용합니다. 엔진 실행 파일은 Git에 포함하지 않으므로 최초 한 번 준비해야 합니다.

```powershell
npm ci
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-engines.ps1
npm run tauri dev
```

앱에서는 다음 순서로 연결합니다.

1. `소스 관리`에서 로컬 폴더 또는 GitHub URL을 선택하고 프로젝트를 엽니다.
2. `코드 읽기`를 누릅니다. 인덱싱과 API·함수·클래스·파일 목록 로드를 한 번에 수행합니다.
3. DB가 필요하면 연결 이름과 소스를 저장한 뒤 `DB 읽기`를 누릅니다.
   - SQLite/SQLite DDL은 파일 또는 디렉터리 경로를 사용합니다.
   - PostgreSQL/MySQL/SQL Server/Oracle 연결 문자열은 해당 읽기 실행에만 사용합니다.
4. 고정 왼쪽 메뉴에서 `개요`, `API`, `코드`, `데이터베이스`, `변경 영향`을 오가며 근거를 확인합니다.

직접 빌드한 설치 파일은 `src-tauri/target/release/bundle/nsis/Backend Visual Map_0.1.0_x64-setup.exe`에 생성되며, 내부 엔진도 함께 포함됩니다.

## 현재 기능

- 로컬 워크스페이스 생성/열기 및 GitHub URL clone 기반 워크스페이스 생성
- SQLite, SQLite DDL, PostgreSQL, MySQL/MariaDB, SQL Server, Oracle DB 메타데이터 프로필 저장
- DB 비밀번호를 워크스페이스 파일에 저장하지 않는 세션 전용 연결 문자열 입력
- 번들된 codebase-memory / database-memory 엔진 확인 및 실행
- 실데이터 인벤토리 기반 grouped/focused 코드 -> DB 비주얼 맵 렌더링
- 고정 내비게이션 기반 전체 구조, API 읽기 경로, 코드, DB 구조, 변경 영향 탐색
- 확정 근거, 후보, 미확인 영역과 분석 범위의 분리 표시

## 개발 확인

```powershell
npm run deadcode
npm test -- --run
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## 엔진 바이너리

내부/릴리스 빌드는 엔진을 `src-tauri/engines`에서 Tauri resource로 포함합니다.

- `src-tauri/engines/codebase-memory-mcp.exe`
- `src-tauri/engines/database-memory.exe`

설치 후 앱은 설치 디렉터리의 bundled resource를 우선 사용하므로 사용자가 PATH를 설정할 필요가 없습니다.

로컬 개발 중에는 `BACKEND_VISUAL_MAP_ENGINE_DIR`로 엔진 폴더를 지정할 수 있습니다.

엔진은 앱 내부 sidecar로만 사용합니다. Codex, Claude, 또는 다른 AI 도구에 MCP 서버로 자동 등록하지 않습니다.

## 설치 파일 범위

```powershell
# 로컬 내부 검증용
npm run build:internal

# 공개 배포용: MIT 라이선스, 공개 엔진, 고지와 dependency inventory를 검증합니다.
npm run tauri build
```

내부 설치본은 실행 중 `internal` 엔진 모드를 표시합니다. 이 저장소는 소스 공개와 로컬 빌드만 제공하며 Windows 설치 파일을 공식 배포하지 않습니다.

### 제품 검증

```powershell
# 실제 Java, C#/.NET, Python/FastAPI + TypeScript monorepo를 고정 commit으로 검증
npm run smoke:code-matrix

# 로컬 검증 설치본의 형식, 엔진, notices, checksum 검증
powershell -File scripts/release-smoke.ps1

```

제품 소스는 MIT로 공개하며 `database-memory v0.1.1` 공개 release를 고정합니다. `.github/workflows/release.yml`은 PostgreSQL 16/MySQL 8.4, 다국어 코드 필드, 공개 Windows 엔진 계약과 업로드하지 않는 로컬 검증용 NSIS 번들을 확인합니다.

## 개인정보와 데이터 접근

앱은 로컬 메타데이터 인덱싱을 기준으로 설계되어 있습니다.

- DB row data를 읽지 않습니다.
- DB 비밀번호/토큰/연결 secret을 워크스페이스 파일에 저장하지 않습니다.
- DB 연결 문자열은 네트워크 DB 인덱싱 실행 중 세션 입력으로만 사용합니다.
- 저장되는 파일은 워크스페이스 설정, engine cache 경로, 인벤토리 스냅샷입니다.
- code-to-DB 관계는 직접 증거가 없는 한 후보(candidate)로 표시합니다.

## 제한사항

- raw full graph를 그대로 렌더링하지 않습니다. 큰 프로젝트는 grouped/focused map으로 축약합니다.
- 외부 DB smoke는 로컬 환경에 해당 DB와 드라이버/연결 문자열이 있을 때만 통과할 수 있습니다.
- SQLite DDL 파서는 DB별 확장 문법 일부를 건너뛰거나 실패할 수 있습니다.
- 공식 Windows 설치 파일 배포는 현재 제품 범위가 아닙니다.
- 현재 제품 목표는 Windows desktop입니다.

## 문서

- [리서치](docs/research/backend-visual-map.md)
- [구현 계획](docs/plans/backend-visual-map.md)
- [제품 완성 계획](docs/plans/backend-visual-map-product-completion.md)
- [3분 데모](docs/demo/backend-visual-map.demo.md)
- [문제 해결](docs/troubleshooting.md)
- [리포트 규칙](docs/reports/README.md)
