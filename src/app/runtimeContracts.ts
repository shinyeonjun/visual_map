import type { EngineRegistry } from "../types/engine";
import type { CodeInventory, DbInventory, Workspace, WorkspaceRecoveryWarning } from "../types/workspace";
import type { InventoryBootstrap, InventorySearchResult, VisualMap } from "../types/visual-map";

type RecordValue = Record<string, unknown>;

/**
 * Tauri is a trust boundary: TypeScript generics do not validate JSON at runtime.
 * Keep these checks deliberately small and structural so a provider can add fields
 * without breaking the UI, while malformed core records fail closed.
 */
export function validateEngineRegistry(value: unknown): EngineRegistry {
  const record = expectRecord(value, "읽기 도구 상태");
  const engines = expectArray(record.engines, "읽기 도구 목록");
  const engineIds = new Set<string>();
  for (const engine of engines) {
    const item = expectRecord(engine, "읽기 도구 항목");
    const id = expectString(item.id, "읽기 도구 ID");
    if (engineIds.has(id)) {
      throw contractError("읽기 도구 ID가 중복됩니다: " + id);
    }
    engineIds.add(id);
    expectString(item.label, "읽기 도구 이름");
    expectString(item.role, "읽기 도구 역할");
    expectString(item.executable, "읽기 도구 실행 파일");
    expectString(item.expectedVersion, "읽기 도구 예상 버전");
    expectString(item.contractVersion, "읽기 도구 계약 버전");
    expectString(item.path, "읽기 도구 경로");
    expectString(item.integrity, "읽기 도구 무결성 상태");
    if (typeof item.available !== "boolean" || typeof item.releasable !== "boolean") {
      throw contractError("읽기 도구 사용 가능 상태 형식이 올바르지 않습니다");
    }
    if (item.sha256 !== undefined && item.sha256 !== null) {
      expectString(item.sha256, "읽기 도구 checksum");
    }
    if (item.error !== undefined && item.error !== null) {
      expectString(item.error, "읽기 도구 오류");
    }
  }
  expectString(record.mode, "읽기 도구 실행 모드");
  expectString(record.engineDir, "읽기 도구 폴더");
  return value as EngineRegistry;
}

export function validateCodeInventory(value: unknown): CodeInventory {
  const record = expectRecord(value, "코드 분석 결과");
  expectString(record.project, "코드 프로젝트 이름");
  const itemIds = new Set<string>();
  for (const key of codeInventoryCollections) {
    const items = expectArray(record[key], `코드 ${key}`);
    items.forEach((item) => {
      const id = validateCodeItem(item, `코드 ${key}`);
      if (itemIds.has(id)) {
        throw contractError(`코드 항목 ID가 중복됩니다: ${id}`);
      }
      itemIds.add(id);
    });
  }
  const summary = expectRecord(record.summary, "코드 분석 요약");
  for (const key of codeInventorySummaryFields) {
    expectNonNegativeInteger(summary[key], `코드 ${key} 요약`);
  }
  if (record.partial !== undefined && typeof record.partial !== "boolean") {
    throw contractError("코드 일부 완료 상태 형식이 올바르지 않습니다");
  }
  expectArray(record.calls, "코드 호출 관계").forEach((call) => {
    const item = expectRecord(call, "코드 호출 관계");
    expectString(item.from, "호출 시작 대상");
    expectString(item.to, "호출 대상");
  });
  if (record.handles !== undefined) {
    expectArray(record.handles, "라우트 핸들러 관계").forEach((handle) => {
      const item = expectRecord(handle, "라우트 핸들러 관계");
      expectString(item.handler, "핸들러 ID");
      expectString(item.route, "라우트 ID");
    });
  }
  if (record.relationGaps !== undefined) {
    expectArray(record.relationGaps, "코드 분석 공백").forEach((gap) => {
      const item = expectRecord(gap, "코드 분석 공백");
      if (item.id !== undefined) {
        expectString(item.id, "코드 분석 공백 ID");
      }
      expectString(item.kind, "분석 공백 종류");
      expectString(item.message, "분석 공백 설명");
    });
  }
  if (record.evidence !== undefined && record.evidence !== null) {
    validateEvidenceSummary(record.evidence, "코드 프로젝트 근거");
  }
  return value as CodeInventory;
}

type CodeIndexContract = {
  workspace: Workspace;
  run: { ok: boolean; stderr: string };
  inventory?: CodeInventory | null;
  inventoryError?: string | null;
};

export function validateCodeIndexResult(value: unknown): CodeIndexContract {
  const record = expectRecord(value, "코드 분석 응답");
  validateWorkspace(record.workspace);
  validateRun(record.run, "코드 분석 실행 결과");
  if (record.inventory !== undefined && record.inventory !== null) {
    validateCodeInventory(record.inventory);
  }
  return value as CodeIndexContract;
}

export function validateDbInventory(value: unknown): DbInventory {
  const record = expectRecord(value, "DB 분석 결과");
  expectString(record.profileId, "DB 프로필 ID");
  const tables = expectArray(record.tables, "DB 테이블 목록");
  const tableIds = new Set<string>();
  tables.forEach((table) => {
    const name = validateDbTable(table);
    const item = table as RecordValue;
    const schema = item.schema === undefined || item.schema === null ? "" : expectString(item.schema, "DB 스키마 이름");
    const id = `${schema}:${name}`;
    if (tableIds.has(id)) {
      throw contractError("DB 테이블 식별자가 중복됩니다: " + id);
    }
    tableIds.add(id);
  });
  for (const key of ["limitRequested", "limitApplied", "resultCount", "totalTables"]) {
    expectOptionalNonNegativeInteger(record[key], "DB " + key);
  }
  if (typeof record.totalTables === "number" && record.totalTables < tables.length) {
    throw contractError("DB 전체 테이블 수가 반환된 테이블 수보다 작습니다");
  }
  if (record.truncated !== undefined && typeof record.truncated !== "boolean" && record.truncated !== null) {
    throw contractError("DB 일부 반환 상태 형식이 올바르지 않습니다");
  }
  if (record.limitClamped !== undefined && typeof record.limitClamped !== "boolean" && record.limitClamped !== null) {
    throw contractError("DB 표시 상한 상태 형식이 올바르지 않습니다");
  }
  return value as DbInventory;
}

type DbIndexContract = {
  workspace: Workspace;
  run: { ok: boolean; stderr: string; stdout?: string };
  indexJson?: unknown | null;
  inventory?: DbInventory | null;
  inventoryError?: string | null;
};

export function validateDbIndexResult(value: unknown): DbIndexContract {
  const record = expectRecord(value, "DB 분석 응답");
  validateWorkspace(record.workspace);
  validateRun(record.run, "DB 분석 실행 결과");
  if (record.inventory !== undefined && record.inventory !== null) {
    validateDbInventory(record.inventory);
  }
  return value as DbIndexContract;
}

type WorkspaceAnalysisContract = {
  workspace: Workspace;
  code: CodeIndexContract | null;
  db: DbIndexContract | null;
  codeError?: string | null;
  dbError?: string | null;
  snapshotSaved: boolean;
};

export function validateWorkspaceAnalysisResult(value: unknown): WorkspaceAnalysisContract {
  const record = expectRecord(value, "통합 분석 응답");
  validateWorkspace(record.workspace);
  if (!("code" in record) || !("db" in record)) {
    throw contractError("통합 분석 응답에 코드 또는 DB 결과가 없습니다");
  }
  if (record.code !== null && record.code !== undefined) {
    validateCodeIndexResult(record.code);
  }
  if (record.db !== null && record.db !== undefined) {
    validateDbIndexResult(record.db);
  }
  if (typeof record.snapshotSaved !== "boolean") {
    throw contractError("통합 분석 snapshot 저장 상태 형식이 올바르지 않습니다");
  }
  return value as WorkspaceAnalysisContract;
}

export function validateInventoryBootstrap(value: unknown): InventoryBootstrap | null {
  if (value === null) {
    return null;
  }
  const record = expectRecord(value, "저장된 분석 결과");
  validateSnapshot(record.snapshot);
  const summary = expectRecord(record.summary, "저장된 분석 결과 요약");
  expectString(summary.workspaceId, "저장된 결과 프로젝트 ID");
  return value as InventoryBootstrap;
}

export function validateVisualMap(value: unknown): VisualMap {
  const record = expectRecord(value, "시각화 결과");
  expectString(record.id, "시각화 결과 ID");
  expectString(record.workspaceId, "시각화 프로젝트 ID");
  expectString(record.mode, "시각화 모드");
  expectString(record.focus, "시각화 초점");
  const nodeIds = new Set<string>();
  expectArray(record.nodes, "시각화 항목").forEach((node) => {
    const item = expectRecord(node, "시각화 항목");
    const id = expectString(item.id, "시각화 항목 ID");
    if (nodeIds.has(id)) {
      throw contractError("시각화 항목 ID가 중복됩니다: " + id);
    }
    nodeIds.add(id);
    expectString(item.kind, "시각화 항목 종류");
    expectString(item.title, "시각화 항목 이름");
    if (item.parentId !== undefined && item.parentId !== null) {
      expectString(item.parentId, "시각화 상위 구조 ID");
    }
    if (item.depth !== undefined && item.depth !== null) {
      expectNonNegativeInteger(item.depth, "시각화 구조 깊이");
    }
    if (item.assignedBy !== undefined && item.assignedBy !== null) {
      expectString(item.assignedBy, "시각화 구조 배정 근거");
    }
    if (item.metrics !== undefined && item.metrics !== null) {
      const metrics = expectRecord(item.metrics, "시각화 항목 지표");
      expectOptionalNonNegativeInteger(metrics.memberCount, "시각화 항목 구성원 수");
      for (const key of ["apiCount", "codeCount", "dbCount"]) {
        expectNonNegativeInteger(metrics[key], `시각화 항목 ${key}`);
      }
      for (const key of ["memberCount", "handlerCount", "serviceCount", "repositoryCount", "inDegree", "outDegree"]) {
        expectOptionalNonNegativeInteger(metrics[key], `시각화 항목 ${key}`);
      }
      for (const key of ["topApi", "topCode", "topDb"]) {
        validateStringArray(metrics[key], `시각화 항목 ${key}`);
      }
      if (metrics.depth !== undefined && metrics.depth !== null) {
        expectNonNegativeInteger(metrics.depth, "시각화 항목 깊이");
      }
    }
    if (item.coverage !== undefined && item.coverage !== null) {
      const coverage = expectRecord(item.coverage, "시각화 항목 커버리지");
      validateStringArray(coverage.languages, "시각화 항목 언어 커버리지");
      if (typeof coverage.hasBlindSpot !== "boolean") {
        throw contractError("시각화 항목 미분석 상태 형식이 올바르지 않습니다");
      }
      if (coverage.hasPartial !== undefined && typeof coverage.hasPartial !== "boolean") {
        throw contractError("시각화 항목 부분 분석 상태 형식이 올바르지 않습니다");
      }
    }
  });
  const edgeIds = new Set<string>();
  expectArray(record.edges, "시각화 관계").forEach((edge) => {
    const item = expectRecord(edge, "시각화 관계");
    const id = expectString(item.id, "시각화 관계 ID");
    if (edgeIds.has(id)) {
      throw contractError("시각화 관계 ID가 중복됩니다: " + id);
    }
    edgeIds.add(id);
    const from = expectString(item.from, "시각화 관계 시작 대상");
    const to = expectString(item.to, "시각화 관계 도착 대상");
    if (!nodeIds.has(from) || !nodeIds.has(to)) {
      throw contractError("시각화 관계가 존재하지 않는 항목을 가리킵니다: " + id);
    }
    expectString(item.kind, "시각화 관계 종류");
    expectOptionalNonNegativeInteger(item.weight, "시각화 관계 가중치");
  });
  expectArray(record.warnings, "시각화 경고").forEach((warning) => expectString(warning, "시각화 경고"));
  return value as VisualMap;
}

export function validateInventorySearchResult(value: unknown): InventorySearchResult {
  const record = expectRecord(value, "검색 응답");
  expectArray(record.hits, "검색 결과").forEach((hit) => {
    const item = expectRecord(hit, "검색 결과 항목");
    expectString(item.group, "검색 결과 그룹");
    const result = expectRecord(item.item, "검색 결과 대상");
    expectString(result.id, "검색 결과 대상 ID");
    expectString(result.kind, "검색 결과 대상 종류");
    expectString(result.name, "검색 결과 대상 이름");
  });
  if (
    typeof record.total !== "number" ||
    !Number.isFinite(record.total) ||
    record.total < 0 ||
    !Number.isInteger(record.total) ||
    typeof record.truncated !== "boolean"
  ) {
    throw contractError("검색 요약 형식이 올바르지 않습니다");
  }
  expectRecord(record.counts, "검색 결과 개수");
  return value as InventorySearchResult;
}

export function validateWorkspace(value: unknown): Workspace {
  const record = expectRecord(value, "프로젝트");
  expectString(record.id, "프로젝트 ID");
  expectString(record.name, "프로젝트 이름");
  expectString(record.repoPath, "프로젝트 경로");
  const profileIds = new Set<string>();
  expectArray(record.dbProfiles, "프로젝트 DB 연결 목록").forEach((profile) => {
    const id = validateDbProfile(profile);
    if (profileIds.has(id)) {
      throw contractError("프로젝트 DB 연결 ID가 중복됩니다: " + id);
    }
    profileIds.add(id);
  });
  if (record.activeDbProfileId !== undefined && record.activeDbProfileId !== null) {
    const activeId = expectString(record.activeDbProfileId, "활성 DB 연결 ID");
    if (!profileIds.has(activeId)) {
      throw contractError("활성 DB 연결이 목록에 없습니다: " + activeId);
    }
  }
  return value as Workspace;
}

export function validateStringArray(value: unknown, label: string): string[] {
  return expectArray(value, label).map((item) => expectString(item, label));
}

export function validateWorkspaceRecoveryWarnings(value: unknown): WorkspaceRecoveryWarning[] {
  return expectArray(value, "프로젝트 복구 경고").map((warning) => {
    const item = expectRecord(warning, "프로젝트 복구 경고");
    expectString(item.workspaceId, "복구 경고 프로젝트 ID");
    expectString(item.kind, "복구 경고 종류");
    expectString(item.message, "복구 경고 설명");
    expectString(item.action, "복구 경고 조치");
    return warning as WorkspaceRecoveryWarning;
  });
}

export function validateWorkspaceList(value: unknown): Workspace[] {
  if (!Array.isArray(value)) {
    throw contractError("프로젝트 목록 형식이 올바르지 않습니다");
  }
  const workspaceIds = new Set<string>();
  value.forEach((workspace) => {
    validateWorkspace(workspace);
    const id = expectString(expectRecord(workspace, "프로젝트").id, "프로젝트 ID");
    if (workspaceIds.has(id)) {
      throw contractError("프로젝트 ID가 중복됩니다: " + id);
    }
    workspaceIds.add(id);
  });
  return value as Workspace[];
}

const codeInventoryCollections = [
  "routes",
  "services",
  "files",
  "handlers",
  "repositories",
  "functions",
  "classes",
  "modules",
  "unknown",
] as const;

const codeInventorySummaryFields = [
  "routes",
  "handlers",
  "services",
  "repositories",
  "functions",
  "classes",
  "modules",
  "files",
  "unknown",
] as const;

function validateCodeItem(value: unknown, label: string): string {
  const item = expectRecord(value, label);
  const id = expectString(item.id, `${label} ID`);
  expectString(item.name, `${label} 이름`);
  expectString(item.kind, `${label} 종류`);
  return id;
}

function validateDbTable(value: unknown): string {
  const table = expectRecord(value, "DB 테이블");
  const name = expectString(table.name, "DB 테이블 이름");
  const columnNames = new Set<string>();
  expectArray(table.columns, "DB 컬럼 목록").forEach((column) => {
    const item = expectRecord(column, "DB 컬럼");
    const columnName = expectString(item.name, "DB 컬럼 이름");
    if (columnNames.has(columnName)) {
      throw contractError("DB 컬럼 이름이 중복됩니다: " + columnName);
    }
    columnNames.add(columnName);
  });
  return name;
}

function validateDbProfile(value: unknown): string {
  const profile = expectRecord(value, "DB 연결");
  const id = expectString(profile.id, "DB 연결 ID");
  expectString(profile.name, "DB 연결 이름");
  expectString(profile.source, "DB 연결 종류");
  expectString(profile.cachePath, "DB 연결 캐시 경로");
  if (profile.passwordStored !== false) {
    throw contractError("DB 연결 비밀번호 저장 상태가 올바르지 않습니다");
  }
  expectOptionalNonNegativeInteger(profile.port, "DB 연결 포트");
  return id;
}

function validateSnapshot(value: unknown): void {
  const snapshot = expectRecord(value, "저장된 분석 결과");
  expectString(snapshot.workspaceId, "저장된 결과 프로젝트 ID");
  expectString(snapshot.savedAt, "저장된 결과 시각");
  expectArray(snapshot.items, "저장된 항목").forEach((item) => {
    const entry = expectRecord(item, "저장된 항목");
    expectString(entry.id, "저장된 항목 ID");
    expectString(entry.kind, "저장된 항목 종류");
    expectString(entry.source, "저장된 항목 출처");
  });
  expectArray(snapshot.links, "저장된 관계").forEach((link) => {
    const entry = expectRecord(link, "저장된 관계");
    expectString(entry.id, "저장된 관계 ID");
    expectString(entry.from, "관계 시작 대상");
    expectString(entry.to, "관계 도착 대상");
  });
  const metadata = snapshot.metadata === undefined ? null : expectRecord(snapshot.metadata, "저장된 결과 메타데이터");
  if (metadata?.evidence !== undefined && metadata.evidence !== null) {
    validateEvidenceSummary(metadata.evidence, "저장된 프로젝트 근거");
  }
}

function validateEvidenceSummary(value: unknown, label: string): void {
  const summary = expectRecord(value, label);
  expectString(summary.schema, `${label} 형식`);
  for (const key of ["factCount", "relationCount", "diagnosticCount", "diagnosticsHidden"] as const) {
    expectNonNegativeInteger(summary[key], `${label} ${key}`);
  }
  expectArray(summary.collectors, `${label} provider`).forEach((collector) => {
    const item = expectRecord(collector, `${label} provider`);
    for (const key of ["id", "capability", "mode", "status"] as const) {
      expectString(item[key], `${label} provider ${key}`);
    }
    for (const key of ["detectedByTotal", "factCount", "relationCount", "diagnosticCount"] as const) {
      expectNonNegativeInteger(item[key], `${label} provider ${key}`);
    }
    validateStringArray(item.detectedBy, `${label} provider 감지 근거`);
  });
  expectArray(summary.diagnostics, `${label} 진단`).forEach((diagnostic) => {
    const item = expectRecord(diagnostic, `${label} 진단`);
    expectString(item.collector, `${label} 진단 provider`);
    expectString(item.message, `${label} 진단 설명`);
  });
}

function validateRun(value: unknown, label: string): void {
  const run = expectRecord(value, label);
  if (typeof run.ok !== "boolean") {
    throw contractError(`${label} 성공 여부 형식이 올바르지 않습니다`);
  }
  expectString(run.stderr, `${label} 오류 출력`);
  if (run.stdout !== undefined && typeof run.stdout !== "string") {
    throw contractError(`${label} 표준 출력 형식이 올바르지 않습니다`);
  }
}

function expectRecord(value: unknown, label: string): RecordValue {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw contractError(`${label} 형식이 올바르지 않습니다`);
  }
  return value as RecordValue;
}

function expectArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw contractError(`${label} 형식이 올바르지 않습니다`);
  }
  return value;
}

function expectString(value: unknown, label: string): string {
  if (typeof value !== "string") {
    throw contractError(`${label} 형식이 올바르지 않습니다`);
  }
  return value;
}

function expectOptionalNonNegativeInteger(value: unknown, label: string): void {
  if (value === undefined || value === null) {
    return;
  }
  if (typeof value !== "number" || !Number.isInteger(value) || !Number.isFinite(value) || value < 0) {
    throw contractError(label + " 형식이 올바르지 않습니다");
  }
}

function expectNonNegativeInteger(value: unknown, label: string): void {
  if (typeof value !== "number" || !Number.isInteger(value) || !Number.isFinite(value) || value < 0) {
    throw contractError(label + " 형식이 올바르지 않습니다");
  }
}

function contractError(message: string): Error {
  return new Error(`분석 결과 계약 오류: ${message}`);
}
