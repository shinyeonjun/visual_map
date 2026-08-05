# 문제 해결

이 문서는 설치 앱을 사용할 때 먼저 보는 운영 안내다. 엔진 내부 장애와 재현 로그는
[엔진 트러블슈팅](troubleshooting/code-memory-engine.md)에 기록한다.

## 앱이 시작되지 않고 `Port 1420 is already in use`가 표시됨

`npm run tauri dev`를 두 번 실행했거나 이전 Vite 프로세스가 남은 상태다.

```powershell
Get-NetTCPConnection -LocalPort 1420 -State Listen | Select-Object OwningProcess
Get-Process -Id <PID>
Stop-Process -Id <PID>
npm run tauri dev
```

PID를 확인하지 않은 채 모든 Node 프로세스를 종료하지 않는다. 다른 개발 서버가 같은
Node 런타임을 사용할 수 있다.

## `읽기 도구가 없습니다` 또는 `엔진 없음`

설치 앱에는 다음 리소스가 함께 들어가야 한다.

```text
engines/code-memory-language.exe
engines/database-memory.exe
engines/provider-bundles/providers-manifest.json
engines/provider-bundles/providers-manifest.sig
```

개발 환경에서는 먼저 빌드 자산을 검증한다.

```powershell
npm run verify:engines
npm run verify:notices
```

특정 개발 엔진 디렉터리를 사용할 때만 `BACKEND_VISUAL_MAP_ENGINE_DIR`를 지정한다.
설치 앱은 엔진을 sidecar로 실행하므로 엔진의 installer/setup/MCP 등록 명령을 따로
실행하지 않는다.

## provider를 찾지 못함

설치 파일에는 모든 provider pack이 포함되지만 실행 시에는 감지된 언어 pack만 앱
캐시에 푼다. 다음을 확인한다.

1. `provider-bundles` catalog와 signature가 설치 리소스에 존재하는가
2. 해당 pack ZIP의 SHA-256이 catalog와 일치하는가
3. 앱 캐시의 pack 디렉터리에 `.ready` marker가 존재하는가
4. 보안 제품이 provider 실행 파일을 격리하지 않았는가

catalog 또는 ZIP이 손상되면 네트워크에서 임의로 대체하지 않고 실패하는 것이 정상이다.

## 분석이 오래 걸리거나 진행률이 멈춘 것처럼 보임

언어 provider 단계는 파일 수가 아니라 분석 단위와 LSP 비용의 영향을 받는다.
`stderr`에서 다음 줄을 확인한다.

```text
scheduler providers jobs=<N> max_parallel=<N> max_weight=<N> memory_budget_mb=<N>
timing stage=provider_merge elapsed_ms=<N> documents=<N> relations=<N>
```

- `jobs`가 크고 진행률이 조금씩 증가하면 분석 중이다.
- `jobs=0`인데 `provider_merge`가 오래 걸리면 대형 캐시 JSON 병합 비용이다.
- 취소 후 provider 프로세스가 남는다면 앱 경로의 문제다. 직접 CLI를 강제 종료한 결과와
  혼동하지 않는다.

대형 저장소의 현재 측정치는
[2026-08-05 POC 보고서](reports/poc-validation-2026-08-05.md)를 기준으로 한다.

## `다시 읽기 필요` 또는 스냅샷 stale

다음 조건이면 이전 스냅샷은 그대로 보여주되 최신 결과로 취급하지 않는다.

- Git revision 또는 dirty 상태가 바뀜
- 코드 파일 지문이 바뀜
- 코드 adapter version이 바뀜
- 엔진 version/checksum이 바뀜
- DB profile 또는 DDL 파일이 바뀜

프로젝트를 다시 읽어 새 snapshot을 생성한다. stale 경고를 숨기거나 이전 snapshot을
현재 결과로 승격하지 않는다.

## 파일 대부분이 포함됐는데 빨간 경고가 표시됨

파일 개수와 의미 분석 완전성은 별도다. 생성물·dependency·대형 파일·빌드 문맥 부족으로
일부 파일이 `excluded`일 수 있다. 경고의 상세 사유에서 다음 상태를 구분한다.

- `indexed`: 분석 결과에 포함
- `excluded`: 명시적 사유로 제외
- `missing`: 분석 대상이었지만 결과가 없음

99% 이상 포함됐더라도 `missing`이 있으면 진단은 유지한다. 다만 UI의 경고 등급은 최신
POC 보고서에 기록된 마감 항목이다.

## API가 `ANY`이거나 Handler가 없음

route 목록과 `HANDLES` 연결은 다르다. framework route를 찾았더라도 실제 handler
심볼을 하나로 확정하지 못하면 `ANY`, 후보 또는 미확인으로 남을 수 있다.

1. 현재 snapshot이 stale인지 확인한다.
2. `language-index.json`의 `framework_relations`에서 `HANDLES`를 확인한다.
3. `diagnostics`에서 dependency/compile context 부족 여부를 확인한다.
4. 같은 저장소를 최신 엔진으로 다시 읽는다.

이 상태를 이름 유사도만으로 확정 연결로 바꾸지 않는다.

## DB 연결 실패

- SQLite/SQLite DDL은 입력 파일이 실제로 존재해야 한다.
- PostgreSQL/MySQL/MariaDB/SQL Server/Oracle은 연결 문자열과 스키마 범위를 확인한다.
- Oracle은 앱과 같은 아키텍처의 Oracle Client 11.2 이상이 필요하다.
- 테스트에는 읽기 전용 계정을 사용한다.
- 앱은 row data를 조회하지 않고 catalog/metadata/DDL만 읽는다.
- 연결 secret은 workspace 또는 snapshot에 저장하지 않는다.

PostgreSQL `SERIAL/BIGSERIAL`은 현재 알려진 P0 결함이 있다. 정확한 재현과 영향은
[PG-SERIAL-001](troubleshooting/code-memory-engine.md#pg-serial-001--postgresql-serialbigserial이-전체-snapshot을-실패시킴)을
참고한다.

## 화면이 비어 있음

1. 프로젝트 폴더가 등록됐는지 확인한다.
2. 코드 읽기가 성공해 snapshot이 생성됐는지 확인한다.
3. 왼쪽 레이어 검색 필터를 비운다.
4. breadcrumb에서 `전체 프로젝트`로 돌아간다.
5. `선택 영역 맞춤`을 실행한다.

현재 UI는 별도 Atlas/Workbench 화면 전환이 아니라 한 캔버스에서
프로젝트 → 패키지 → 모듈 → 코드 순으로 펼치는 구조다.

## 캐시가 계속 커짐

현재 버전은 코드 cache를 workspace별로 격리하고 현재와 이전의 완전한 generation만
보존한다. workspace를 앱에서 삭제하면 해당 코드·DB cache도 같이 삭제된다.

현재 기본 위치:

```text
%LOCALAPPDATA%\VisualMap\workspaces\<workspace-id>\engines\
  codebase-memory\0.1.0\contract-1\cache
```

구버전이 만든 아래 전역 경로는 현재 앱이 더 이상 사용하거나 늘리지 않지만, rollback과
진단 자료를 보호하기 위해 자동 삭제하지 않는다.

```text
%LOCALAPPDATA%\VisualMap\cache\code-memory
```

모든 workspace를 최신 버전으로 다시 읽었고 구버전 rollback이 필요 없을 때만 앱을 완전히
종료한 뒤 위의 **정확한 구버전 디렉터리**를 수동 삭제한다. `VisualMap` 전체나
`workspaces` 디렉터리를 삭제하면 프로젝트 설정과 현재 snapshot도 사라지므로 지우지 않는다.

## 설치 파일 빌드

```powershell
# 내부 Demo 빌드
npm run build:internal

# 공개 release-ready manifest가 준비된 뒤에만 사용
npm run tauri build
```

내부 installer는 재배포용 공개 릴리스가 아니다. 공개 빌드는 engine manifest의
`releaseReady`, 라이선스, checksum과 서명 검증을 모두 통과해야 한다.
