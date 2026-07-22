# 문제 해결

## 엔진 없음

상태 바에 `엔진 없음`이 표시되면 아래를 확인하세요.

- 설치 앱: 설치 디렉터리 아래 `engines/codebase-memory-mcp.exe`, `engines/database-memory.exe`가 있어야 합니다.
- 개발 앱: 기본 위치는 앱 데이터의 `engines` 디렉터리입니다.
- 개발 앱에서 다른 엔진 폴더를 쓰려면 `BACKEND_VISUAL_MAP_ENGINE_DIR` 환경 변수를 지정하세요.
- 엔진 installer/setup/MCP registration 명령은 실행하지 마세요. 앱은 엔진을 내부 sidecar로만 사용합니다.

## 설치 파일 빌드

설치 패키징에는 실제 엔진 바이너리가 필요합니다. placeholder 실행 파일은 의도적으로 포함하지 않습니다.

```powershell
# 내부 사용 전용: 공개되지 않은 개발 DB 엔진을 명시적으로 허용합니다.
npm run build:internal

# 공개 배포: release-ready 엔진과 제품 배포 라이선스가 모두 필요합니다.
npm run tauri build
```

NSIS installer는 `src-tauri/target/release/bundle/nsis` 아래에 생성됩니다.
`build:internal` 산출물은 재배포하지 마세요. 앱은 `internal` 모드로 실행되며 DB 엔진을 `development-internal`로 표시합니다.

## GitHub clone 실패

- GitHub URL은 `https://github.com/owner/repo` 또는 `git@github.com:owner/repo` 형식을 사용하세요.
- 앱은 URL을 직접 인덱싱하지 않고 먼저 로컬 workspace repo 디렉터리로 clone합니다.
- private repo는 현재 Windows 사용자 환경의 Git 인증 상태가 필요합니다.
- clone 실패 후에는 같은 이름의 워크스페이스를 다시 만들기 전에 경로와 인증 상태를 확인하세요.

## DB 연결 실패

- SQLite/SQLite DDL은 파일 경로가 실제로 존재해야 합니다.
- PostgreSQL/MySQL/SQL Server/Oracle은 연결 문자열을 인덱싱 실행 때마다 입력해야 합니다.
- Oracle은 별도 Oracle Client 11.2 이상이 필요합니다. Windows에서는 앱과 같은 아키텍처의 Instant Client를 설치하고 `oci.dll`이 있는 폴더를 `PATH`에 추가한 뒤 앱을 완전히 다시 시작하세요.
- 앱은 DB 비밀번호나 연결 secret을 워크스페이스 파일에 저장하지 않습니다.
- DB driver, ODBC provider, 방화벽, 네트워크 접근 권한을 확인하세요.
- 앱은 row data를 조회하지 않고 catalog/metadata/DDL/engine graph output만 사용합니다.

## 스냅샷이 오래됨

스냅샷이 stale로 표시되면 이전 화면을 최신처럼 보여주지 않습니다.

- 코드 저장소 경로가 바뀌었으면 `저장소 인덱싱`과 `코드 불러오기`를 다시 실행하세요.
- DB 프로필이나 DDL 파일이 바뀌었으면 `메타데이터 인덱싱`과 `테이블 불러오기`를 다시 실행하세요.
- 새 인벤토리를 불러오면 앱이 새 snapshot을 저장하고 map을 다시 생성합니다.

## 화면이 비어 있음

- Workbench에서 코드 또는 DB 인벤토리를 먼저 불러오세요.
- Atlas는 read-only 탐색 공간이므로 setup은 Workbench에서 진행하세요.
- canvas가 빈 상태일 때 fake/current-looking data를 표시하지 않는 것이 정상 동작입니다.

## 알려진 제한

- raw full graph는 렌더링하지 않습니다.
- code-to-DB 링크는 직접 증거가 없는 한 candidate입니다.
- 이전 스냅샷의 CALLS에 엔진 신뢰도 점수가 없으면 확정 관계에서 제외되며 `코드 읽기`를 다시 실행해야 합니다.
- 외부 DB smoke는 해당 DB 인스턴스와 드라이버가 없는 환경에서는 skip됩니다.
- 공식 Windows 설치 파일 배포와 코드 서명은 현재 제품 범위가 아닙니다.
