# Visual Map 설치 레이아웃 계약

상태: 제안 확정안

목표는 Visual Map이 사용자의 개발환경을 수정하지 않고, 앱 자체의 설정·분석 도구·캐시만 관리하는 것이다.

## 설치 위치

앱은 관리자 권한 없이 설치할 수 있도록 사용자 영역에 설치한다.

```text
%LOCALAPPDATA%\Programs\VisualMap\
  VisualMap.exe
  resources\
```

사용자 설정과 분석 데이터는 앱 실행 파일과 분리한다.

```text
%LOCALAPPDATA%\VisualMap\
  config\
  projects\
  cache\
  logs\
  temp\
```

## 처음 설치할 때 생성되는 것

기본 설치에서는 다음만 설치한다.

```text
VisualMap.exe                               앱 본체
resources\engines\provider-bundles\       서명된 provider catalog
config\settings.toml                       사용자 설정 기본값
logs\                                      오류 로그 폴더
```

프로젝트를 연결하기 전에는 프로젝트별 분석 결과나 언어별 대형 도구를
강제로 만들지 않는다.

## 언어 provider

프로젝트에서 언어가 감지된 경우에만 필요한 provider를 준비한다.

```text
%LOCALAPPDATA%\VisualMap\cache\provider-roots\<catalog-hash>\
  node\
  python\
  java\
  clang\
  dotnet\
  go\
  rust\
  php\
  ruby\
  dart\
  manifest.json
  .packs\<pack-id>.ready
```

catalog의 Ed25519 서명을 앱에 고정된 공개키로 먼저 검증한 뒤, 감지된 언어의
pack만 HTTPS로 내려받는다. 각 ZIP의 크기와 SHA-256, 해제 크기와 핵심
entrypoint의 SHA-256을 검증한 뒤 같은 볼륨에서 원자적으로 활성화한다.
폐기 목록에 포함된 버전은 기존 캐시에 있어도 실행하지 않는다.

```text
cache\provider-downloads\<pack-sha256>.zip
cache\provider-roots\<catalog-hash>\java\
cache\provider-roots\<catalog-hash>\.packs\java.ready
```

provider는 시스템 PATH나 레지스트리에 등록하지 않는다. Rust bridge가
절대 경로로 실행하고, 필요한 환경변수도 해당 자식 프로세스에만 적용한다.

## 프로젝트 연결 후 생성되는 것

```text
%LOCALAPPDATA%\VisualMap\projects\<project-id>\
  project.json       프로젝트 경로와 감지 결과
  environment.json   선택된 SDK/provider 경로와 버전
  indexes\           language-index와 architecture-index
  diagnostics.json   분석 오류와 provider 정보
```

프로젝트 원본의 다음 파일은 복사하거나 수정하지 않는다.

```text
build.gradle, pom.xml, go.mod, package.json, pyproject.toml,
Gemfile, pubspec.yaml, Cargo.toml, CMakeLists.txt
```

이 파일들은 프로젝트에서 읽기만 한다.

## 의존성 캐시

Gradle, Maven, Go module, Python, Ruby, Dart 등의 다운로드 결과는
프로젝트를 수정하지 않고 Visual Map 전용 캐시에 저장한다.

```text
%LOCALAPPDATA%\VisualMap\cache\
  gradle\
  maven\
  go\
  python\
  ruby\
  dart\
  npm\
```

캐시 경로는 시스템 환경변수로 등록하지 않는다. 분석 프로세스를 실행할
때만 자식 프로세스 환경으로 전달한다.

## 사용자 환경 사용 우선순위

```text
프로젝트가 지정한 환경
  → 사용자의 기존 설치 환경
  → Visual Map 내부 provider
  → 구조 분석만 수행
```

사용자의 JDK, Python, Go, Ruby, Dart, Clang과 가상환경은 변경하지 않는다.
Visual Map provider가 있어도 사용자 환경을 덮어쓰지 않는다.

## 설치하지 않는 것

- 시스템 PATH와 레지스트리 변경
- 사용자의 JDK·Python·Go·Ruby·Dart·Clang 덮어쓰기
- 프로젝트 build 설정파일 복사·수정
- 프로젝트 가상환경 삭제·수정
- 프로젝트 의존성의 전역 설치
- WSL, Docker, 별도 서버

## 설치 모드

기본 모드는 온라인 로컬 설치로 한다.

- 기본 설치: 앱 본체만 설치
- 프로젝트 연결: 필요한 provider만 앱 전용 폴더에 준비
- 오프라인 설치: 모든 provider를 포함한 별도 대형 설치 파일 제공

온라인 모드에서도 사용자는 별도의 언어 도구 설치 프로그램을 실행하지
않는다. provider 준비는 Visual Map이 자체 폴더에서 처리한다.

## 삭제

삭제 시 앱 실행 파일과 Visual Map이 만든 provider·cache·log만 삭제한다.
프로젝트 원본, 사용자의 기존 개발환경, 기존 SDK는 삭제하지 않는다.
프로젝트 분석 결과와 캐시 삭제는 별도 확인을 받는다.
