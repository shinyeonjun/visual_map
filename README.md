# Backend Visual Map

백엔드 코드와 관계형 데이터베이스 메타데이터의 관계를 **근거와 함께** 탐색하는 Windows 우선 Tauri + React 데스크톱 앱입니다.

![Backend Visual Map workbench](docs/reports/screenshots/ui-light-default/workbench.png)

## Why it exists

큰 백엔드 저장소에서 "이 API가 어떤 테이블과 컬럼에 영향을 주는가?"를 코드·스키마를 오가며 추적하는 비용을 줄입니다. raw dependency graph를 그대로 던지지 않고, Workbench와 Atlas에서 API Flow, Table Usage, Column Impact를 focused view로 보여줍니다.

**Product boundary:** row data는 읽지 않고, DB 비밀번호·토큰·연결 secret은 workspace 파일에 저장하지 않습니다.

## What it does

- local folder 또는 GitHub URL로 workspace 생성 후 코드 인덱싱
- SQLite/DDL, PostgreSQL, MySQL/MariaDB, SQL Server, Oracle 메타데이터 인덱싱
- API Flow, Table Usage, Column Impact를 evidence-backed map으로 탐색
- bundled codebase/database engine을 sidecar로 실행

## Quick start

1. Windows에서 source를 build하거나 내부 검증용 installer를 생성합니다.
   - 이 저장소는 source와 local build를 공개하며, 공식 installer binary를 배포하지 않습니다.
   - `npm run build:internal`은 내부 검증용 installer를 `src-tauri/target/release/bundle/nsis/`에 생성합니다.
   - 설치 앱에는 `codebase-memory-mcp.exe`와 `database-memory.exe`가 내부 엔진으로 포함됩니다.
2. 앱을 열고 Workbench에서 워크스페이스를 만듭니다.
   - 로컬 폴더: 저장소 전체 경로를 입력합니다.
   - GitHub URL: 앱이 먼저 로컬로 clone한 뒤 그 로컬 복사본을 인덱싱합니다.
3. `저장소 인덱싱`을 누른 뒤 `코드 불러오기`를 누릅니다.
4. DB 프로필을 저장합니다.
   - SQLite/SQLite DDL은 파일 경로를 사용합니다.
   - PostgreSQL/MySQL/SQL Server/Oracle 연결 문자열은 해당 세션의 인덱싱 실행에만 사용합니다.
5. `메타데이터 인덱싱`을 누른 뒤 `테이블 불러오기`를 누릅니다.
6. Workbench 또는 Atlas에서 overview/API Flow/Table Usage/Column Impact를 확인합니다.

## Develop and verify

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo test
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

# 현재 release-candidate installer의 형식, 엔진, notices, checksum 검증
powershell -File scripts/release-smoke.ps1

```

제품 소스는 MIT로 공개하며 `database-memory v0.1.1` 공개 release를 고정합니다. `.github/workflows/release.yml`은 PostgreSQL 16/MySQL 8.4, 다국어 코드 필드와 공개 Windows 엔진 계약을 검증합니다.

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
