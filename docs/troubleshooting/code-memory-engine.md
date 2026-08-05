# 엔진 트러블슈팅 기록

상태 기준일: 2026-08-05
검증 commit: `340be023dc226b597e6f12016b70a6aeb78cb5af`
검증 branch: `agent/local-first-optimization`

이 문서는 장애를 회고 문장으로 설명하지 않는다. 각 항목에 재현 조건, 원문 로그,
근본 원인, 실제 수정 코드, 검증 결과와 남은 경계를 남긴다. 과거 개발 일지는
[원본 이력](CODE_MEMORY_TROUBLESHOOTING_HISTORY.md)에 보존했지만 현재 완료 판정에는
사용하지 않는다.

## 기록 형식

새 항목은 아래 순서를 지킨다.

```text
ID / 상태 / 최초·최종 확인 commit
재현 조건과 명령
원문 로그
근본 원인
수정 코드 또는 아직 수정하지 않은 코드
검증 명령과 결과
남은 경계
```

## OPEN

### PG-SERIAL-001 — PostgreSQL `SERIAL/BIGSERIAL`이 전체 snapshot을 실패시킴

상태: **P0 / 미수정 / 재현됨**

재현 조건:

- PostgreSQL `16.14`
- `public.users.id SERIAL PRIMARY KEY`
- `database-memory 0.2.0`, contract `2`

```sql
CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  email TEXT NOT NULL UNIQUE
);
```

```powershell
& .\src-tauri\engines\database-memory.exe index `
  --source postgres `
  --connection-string '<disposable-test-connection>' `
  --alias poc-postgres16-serial `
  --schema public `
  --format json `
  --cache-path .\db-postgres16-serial.sqlite
```

실제 로그:

```text
error: {"error":{"analysis_failure":{"code":"metadata_mapping_failed",
"message":"duplicate canonical metadata relationship USES_SEQUENCE:
postgres:poc-postgres16-serial:visualmap_poc:public:column:users:id->
postgres:poc-postgres16-serial:visualmap_poc:public:sequence:users_id_seq",
"retryable":false,"stage":"mapping"},"code":"analysis_failed"},"status":"failed"}
exit-code=1
```

실패 후 `list-snapshots` 결과는 빈 배열이었다. 부분 snapshot을 저장하지 않는
complete-or-failed 계약은 지켜졌다.

catalog 원문:

```text
default|users|id|users_id_seq|n
owned-by|users|id|users_id_seq|a
```

근본 원인:

`db_memory/crates/database-memory-core/src/adapters/postgres_catalog/reading/dependencies.rs`
의 `read_sequence_usages`가 sequence ownership과 `nextval(...)` default dependency를
`UNION`한다. 네 번째 컬럼 `dependency_type`이 각각 `a`, `n`이라 SQL 행은 중복이
아니지만, mapper는 둘 다 같은 `USES_SEQUENCE(column -> sequence)` 관계로 바꾼다.

현재 문제 코드:

```rust
SELECT dep.refobjid, dep.refobjsubid, seq.oid, dep.deptype
// sequence ownership: deptype a/i
UNION
SELECT attrdef.adrelid, attrdef.adnum, seq.oid, dep.deptype
// nextval default dependency: deptype n/a/i
```

`reading/mapper.rs`는 dependency type을 property에만 넣고 관계 identity는 동일하게
생성한다.

```rust
metadata.relationships.push(MetadataRelationship {
    kind: MetadataRelationshipKind::UsesSequence,
    from_key: column.clone(),
    to_key: sequence.clone(),
    ordinal: None,
    properties,
});
```

`mapping/relationships.rs`는 `kind/from/to/ordinal`이 같은 관계를 fail-closed로
거부한다. 이 검증은 완화하지 않는다. 수정 위치는 PostgreSQL sequence usage를 하나의
정규 관계로 만드는 reader 또는 mapper다.

검증 영수증:

```text
D:\visual_map_reliability_lab\_results\poc-audit-20260805-1430\
  db-postgres16-serial-repro-20260805.sqlite
```

완료 조건:

1. `SERIAL`, `BIGSERIAL`, identity column을 각각 live PostgreSQL에서 검증
2. column당 `USES_SEQUENCE` 한 건
3. dependency 종류는 정규화된 property로 보존
4. 기존 PostgreSQL live identity 테스트와 전체 DB 테스트 통과

### CODE-SCALE-001 — persistent JSON은 제거됐지만 대형 결과 materialization이 메모리를 사용

상태: **P1 / 저장 경로 해결, 메모리 경로 미해결 / 재현됨**

재현 조건:

- Ghost `ee5529727040f3863682b7c8aa8aef70d4fbc20a`
- 동일 source/config/provider checksum으로 연속 실행
- 언어 결과 JSON `306,720,304 bytes`

실제 hot-run 로그:

```text
cached TypeScript/JavaScript project model
scheduler providers jobs=0 max_parallel=1 max_weight=4 memory_budget_mb=12271
timing stage=provider_merge elapsed_ms=18415 documents=5550 relations=177570 diagnostics=44
cached framework analysis
timing stage=framework_analysis elapsed_ms=89 frameworks=4 framework_relations=345 diagnostics=44
timing stage=architecture_and_json elapsed_ms=209 cached=true key=4e2f2f4adb43d94b
```

provider는 실행되지 않았지만 cached language JSON 역직렬화·병합·재직렬화 비용이 남았다.
architecture SHA-256은 cold 결과와 일치해 결정성은 유지됐다. 이 로그는 변경 전 기준선이다.

영수증:

```text
D:\visual_map_reliability_lab\_results\poc-audit-20260805-1430\javascript-ghost\
  hot.stderr.log
  hot.language-index.json
  hot.architecture-index.json
```

수정한 persistent 경로:

```text
code-memory.generation-receipt.v1
  -> generations/<id>/code-graph.sqlite
     schema code-memory.graph-store.v3
     512-row GZip chunks + thin indexes
  -> current.json / previous.json
```

Plane에서 같은 `76,597 nodes / 12,148 CALLS / 620 HANDLES`를 유지하면서 generation
SQLite가 `147,390,464`에서 `95,711,232 bytes`로 35.1% 줄었다. warm run은
11.302초였고 staging 0개, legacy result JSON 0개, complete generation 2개였다.

그러나 warm run의 엔진 부모 peak working set은 `1,136,390,144 bytes`였다. SQLite
쓰기 전에 inventory, relations, architecture를 한 프로세스에서 완성하는 현재 pipeline
때문이다. 저장소 포맷만으로 이 메모리를 줄였다고 주장하지 않는다.

남은 완료 조건은 provider 재실행 없는 hot run의 p95와 전체 process-tree memory를 release
빌드에서 측정하고, projection별 streaming/청크 병합으로 부모 process peak를 줄이는 것이다.
결과 필드를 삭제해 크기를 줄이지 않는다.

### CODE-DART-001 — Serverpod 전체 monorepo가 78개 분석 단위에서 실용 시간 초과

상태: **P1 / 대형 범위 미검증**

재현 조건:

- Serverpod `3a6e8460378d5502431323adc8dbfb69953f2316`
- 전체 저장소 root 분석

실제 로그:

```text
scheduler providers jobs=78 max_parallel=4 max_weight=4 memory_budget_mb=12029
@visual-map-progress {"completed":35,"stage":"providers"}
...
@visual-map-progress {"completed":37,"stage":"providers"}
```

장시간 실행 후 운영 POC를 중단했다. 같은 코드베이스의 작은 auth server package는
8.5초에 16/16 파일, endpoint 13건, `HANDLES` 11건으로 통과했다. 따라서 Dart
provider 자체 실패가 아니라 monorepo 분석 단위와 LSP session 비용 문제다.

직접 CLI를 `Ctrl+C`로 중단했을 때 child Dart process가 남아 PID를 확인한 뒤 종료했다.
Tauri 앱 경로는 Windows Job Object로 process tree를 정리하며 별도 테스트에서 잔류
process 0건을 확인했다. 두 실행 경로를 같은 결과로 기록하지 않는다.

완료 조건:

1. 같은 Dart SDK context를 공유할 수 있는 단위를 묶음
2. package별 timeout과 전체 deadline을 모두 표시
3. 전체 Serverpod cold/hot 실행 시간과 잔류 process 0건 확보

### CODE-FRAMEWORK-001 — route inventory와 handler 연결 품질이 framework마다 다름

상태: **P1 / 부분 지원**

2026-08-05 실저장소 결과:

| 저장소                         | Endpoint | HANDLES | 해결률 |
| ------------------------------ | -------: | ------: | -----: |
| NestJS boilerplate             |       24 |      24 |   100% |
| FastAPI full-stack             |       23 |      23 |   100% |
| Spring Petclinic microservices |       16 |      16 |   100% |
| C# CleanArchitecture           |       10 |      10 |   100% |
| SimpleBank Go                  |       13 |      12 |  92.3% |
| Bagisto Laravel                |       14 |      12 |  85.7% |
| Serverpod auth package         |       13 |      11 |  84.6% |
| Drogon C++                     |       10 |       4 |  40.0% |
| Vaultwarden Rocket             |      305 |     210 |  68.9% |
| Redmine Rails                  |    1,221 |     834 |  68.3% |
| Ghost JS/TS                    |      340 |       4 |   1.2% |

같은 `지원 언어`라도 route-to-handler 품질은 동일하지 않다. UI와 문서는 이 표를
무시하고 12개 언어를 동일 완성도로 표시하면 안 된다. 각 framework의 누락 fixture를
추가하고 실제 저장소 결과가 개선될 때만 product-validated 범위를 넓힌다.

### CACHE-001 — 과거 전역 cache가 남아 있음

상태: **P2 / 신규 누적 해결, 과거 데이터 자동 삭제 안 함**

실측:

```text
%LOCALAPPDATA%\VisualMap             6,913,178,103 bytes (6.44 GiB)
%LOCALAPPDATA%\VisualMap\cache       6,598,007,048 bytes (6.14 GiB)
largest code-memory project cache    1.51 GiB
```

현재 Tauri는 `CBM_CACHE_DIR`와 `CODE_MEMORY_CACHE_ROOT`를 workspace별 경로로 넘긴다.

```text
%LOCALAPPDATA%\VisualMap\workspaces\<workspace-id>\engines\
  codebase-memory\0.1.0\contract-1\cache
```

workspace 삭제는 이 디렉터리를 함께 삭제한다. `code_memory/rust/src/cache.rs`는 내부에서
current와 previous complete generation이 참조하는 cache만 보존한다.

```rust
let mut retained = manifest.files.iter().cloned().collect::<HashSet<_>>();
if let Some(previous) = previous_current.as_ref() {
    retained.extend(previous.files.iter().cloned());
}
prune_managed_cache_files(cache_base, &known, &retained);
```

참조가 끝난 16진 project cache 디렉터리와 LSP workspace도 함께 제거한다. 따라서 새 앱
경로는 전역 orphan을 만들지 않는다.

기존 `%LOCALAPPDATA%\VisualMap\cache\code-memory`의 6.14 GiB는 구버전 rollback이나
진단에 필요할 수 있어 자동 삭제하지 않는다. 최신 버전으로 모든 workspace를 다시 읽고
구버전 rollback이 필요 없을 때 앱을 종료한 뒤 사용자가 해당 **정확한 디렉터리만** 한 번
삭제할 수 있다. 장기 사용에서 workspace별 runtime cache가 실제로 다시 과도하게 커질 때만
LRU/용량 예산을 추가한다.

## RESOLVED

### STORAGE-001 — 앱 hot path가 대형 JSON과 중복 row payload를 계속 저장함

상태: **해결됨 / persistent storage 기준**

재현 조건:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File scripts\measure-storage-poc.ps1 `
  -Cases plane
```

첫 SQLite prototype의 Plane 결과:

```text
nodes=76597 calls=12148 handles=620 architectureNodes=9538 architectureEdges=16275
generation database  147,390,464 bytes
two-generation store 312,316,729 bytes
```

근본 원인은 검색용 열과 완전한 JSON row를 레코드마다 중복 저장해 JSON key와 SQLite row
overhead를 매번 지불한 것이다. 그 전 앱 경로는 대형 `language-index.json`과 snapshot
JSON/ZIP도 통째로 읽고 다시 썼다.

수정 코드의 저장 단위:

```rust
const CHUNK_SIZE: usize = 512;

for (chunk_index, chunk) in rows.chunks(CHUNK_SIZE).enumerate() {
    statement.execute(params![kind, chunk_index, chunk.len(), json_blob(chunk)?])?;
}
```

`json_blob`은 `Compression::fast()` GZip을 사용하고, 이름·경로·관계 endpoint처럼 검색에
필요한 필드만 얇은 SQLite index에 둔다. Tauri snapshot도 같은 512개 청크 원칙을 쓰며
검색은 결과가 들어 있는 청크만 푼다. generation은 staging에서 완성·sync한 뒤 rename하고
`current.json`을 교체한다.

최종 검증:

```text
Plane generation database   95,711,232 bytes (-35.1%)
Plane two-generation store 208,944,185 bytes (-33.1%)
cold/warm counts            identical
complete generations        2
staging directories         0
legacy result JSON          0

SimpleBank generation DB     5,939,200 bytes
SimpleBank warm                  7.354 s
```

원시 영수증:

```text
D:\visual_map_reliability_lab\_results\storage-poc-20260805-chunked\storage-summary.json
D:\visual_map_reliability_lab\_results\storage-poc-20260805-final\storage-summary.json
```

persistent storage는 해결됐지만 분석 도중의 전체 in-memory materialization은
`CODE-SCALE-001`에 남아 있다.

### SQLITE-PATH-001 — Windows 긴 workspace 경로에서 SQLite를 만들지 못함

상태: **해결됨**

실제 FastAPI 저장소를 Tauri ignored field test로 실행했을 때 엔진 분석은 끝났지만 publish가
실패했다.

```text
cannot create code generation database: unable to open database file
```

Tauri workspace cache 아래의 staging DB 경로가 Windows 일반 경로 한도보다 길었다. Rust
filesystem은 디렉터리를 만들었지만 SQLite가 받은 비정규화 경로는 열지 못했다.

공통 open 지점에서 존재하는 파일 또는 부모 디렉터리를 canonicalize한다. Windows의
canonical path는 verbatim 경로가 되므로 파일명만 다시 붙여 SQLite에 전달한다.

```rust
fn sqlite_database_path(path: &Path) -> PathBuf {
    if path.is_file() {
        return fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    path.parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .and_then(|parent| path.file_name().map(|name| parent.join(name)))
        .unwrap_or_else(|| path.to_path_buf())
}
```

코드 generation store, DB graph store와 SQLite source reader, Tauri snapshot store의
실제 SQLite open에 적용했다.

검증:

```text
sqlite_generation_supports_windows_long_paths       passed (path >= 280 chars)
sqlite_storage_and_source_support_windows_long_paths passed (path >= 280 chars)
snapshot_database_supports_windows_long_paths       passed (path >= 280 chars)
code_field_fastapi_adapter_proves_real_import_calls passed, static import calls=3
```

### POC-HARNESS-001 — PowerShell args BOM과 `Start-Process` 종료 코드가 측정을 막음

상태: **해결됨**

첫 storage POC 실행의 실제 오류:

```text
invalid cli args line 1 column 1
```

PowerShell `Set-Content -Encoding utf8`이 환경에 따라 JSON 앞에 BOM을 붙였고 engine의 strict
JSON parser가 첫 byte에서 거부했다. 다음 실행에서는 redirected `Start-Process`가 완전한
receipt와 SQLite를 남겼지만 `.ExitCode`가 `null`인 상태를 harness가 실패로 처리했다.

수정:

```powershell
[IO.File]::WriteAllText($argsPath, $payload, [Text.UTF8Encoding]::new($false))
$process.WaitForExit()
$process.Refresh()
if ($null -ne $process.ExitCode -and [int]$process.ExitCode -ne 0) { throw ... }
```

종료 코드가 제공되지 않는 경우도 성공으로 추정하지 않는다. stdout의 마지막
`code-memory.generation-receipt.v1`을 찾아 `status=complete`, SQLite magic, generation 2개,
staging 0개, legacy JSON 0개를 모두 확인해야 POC가 통과한다.

최종 실행:

```text
PASS simplebank cold=50222ms warm=7354ms sqlite=5939200B
Wrote D:\visual_map_reliability_lab\_results\storage-poc-20260805-final\storage-summary.json
```

### TAURI-JSON-001 — 정상 architecture JSON이 secret redaction으로 손상됨

상태: **해결됨**

재현 조건은 Plane architecture에 `forgot-password:module`처럼 secret key 문자열과
닮은 정상 코드 경로가 포함되고, 약 12.8MB JSON을 Tauri가 수신하는 경우였다.

실제 사용자 로그:

```text
새 코드 인덱스를 검증하지 못했습니다:
코드 엔진 get_architecture 응답이 올바른 JSON이 아닙니다
```

근본 원인은 구조를 모르는 문자열 redaction이 JSON 내부 코드를 key/value secret으로
오인해 payload를 변형한 것이다.

수정 코드:

```rust
fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                if SECRET_KEYS
                    .iter()
                    .any(|secret_key| key.eq_ignore_ascii_case(secret_key))
                {
                    *value = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        serde_json::Value::String(text) => {
            *text = redact_unstructured_secrets(text);
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}
```

`engine_json_value`는 BOM과 engine log framing을 허용하되 정확히 하나의 완전한 JSON
value만 수락한다.

검증:

```text
engine_json_value_accepts_log_prefixed_json_line
engine_json_value_accepts_pretty_json_between_engine_logs
engine_json_value_accepts_bom_and_same_line_prefix
engine_json_value_rejects_ambiguous_log_payloads
src-tauri 전체: 288 passed, 0 failed, 5 ignored (293 total)
```

### TAURI-URL-001 — `http://` 절대 URL이 분석 thread panic을 일으킴

상태: **해결됨**

실제 사용자 로그:

```text
코드 분석 작업이 비정상 종료되었습니다
```

원인은 `http://`를 `https://`와 같은 8바이트 접두사로 취급해 잘못된 byte index로
slice한 것이다.

현재 수정 코드:

```rust
let authority_start = value
    .strip_prefix("https://").map(|_| 8)
    .or_else(|| value.strip_prefix("http://").map(|_| 7));
```

검증:

```text
normalize_url_path("http://localhost:8000/api/items") == Some("/api/items")
normalize_url_path("https://example.com/api/items?limit=1") == Some("/api/items")
normalize_url_path("http://localhost:8000") == None
```

### SNAPSHOT-001 — 분석 규칙 변경 뒤 과거 cache/snapshot이 재사용됨

상태: **해결됨**

증상은 엔진 코드를 고친 뒤에도 과거 route·CALLS 결과가 계속 표시되는 것이었다.
source checksum만으로는 adapter 알고리즘 변경을 감지할 수 없었다.

현재 수정 코드:

```rust
const CODE_ADAPTER_VERSION: &str = "7";

if code.adapter_version.as_deref() != Some(CODE_ADAPTER_VERSION) {
    push_unique(&mut reasons, "코드 분석 규칙이 바뀌어 다시 읽어야 합니다");
}
```

검증:

```text
adapter version이 다르면 snapshot stale reason 생성
새 snapshot metadata에 adapter_version=7 저장
구버전 snapshot은 자동으로 현재 결과로 승격하지 않음
```

### PROCESS-001 — 앱 취소 시 provider child process가 남음

상태: **Tauri 경로 해결됨 / 직접 CLI 강제 종료는 별도 경계**

Windows 앱 실행에서는 child process를 Job Object에 붙이고 job handle이 닫힐 때 전체
process tree를 종료한다.

```rust
limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
AssignProcessToJobObject(job.as_raw_handle() as _, child.as_raw_handle() as _);
```

명시적 취소는 `TerminateJobObject`를 사용하고, Job Object 생성이 불가능한 환경에서만
`taskkill /T /F`로 폴백한다. 실제 Spring 분석 취소·종료 검증에서 residual provider
process는 0건이었다.

## 현재 회귀 명령

```powershell
cargo test --locked --manifest-path code_memory\rust\Cargo.toml
cargo test --locked --manifest-path db_memory\Cargo.toml
cargo test --locked --manifest-path src-tauri\Cargo.toml
npm test -- --run
npm run typecheck
npm run lint
npm run deadcode
npm run build
npm run smoke:rdb
npm run verify:engines

# production app storage cold/hot, generation/legacy cleanup 검증
powershell -NoProfile -ExecutionPolicy Bypass `
  -File scripts\measure-storage-poc.ps1 -Cases simplebank
```

실저장소 POC와 원시 산출물은
[2026-08-05 POC 검증 보고서](../reports/poc-validation-2026-08-05.md)를 기준으로 한다.
