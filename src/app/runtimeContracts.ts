import type { EngineRegistry } from "../types/engine";
import type { CodeInventory, DbInventory, Workspace } from "../types/workspace";
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
  for (const engine of engines) {
    const item = expectRecord(engine, "읽기 도구 항목");
    expectString(item.id, "읽기 도구 ID");
    expectString(item.role, "읽기 도구 역할");
    expectString(item.path, "읽기 도구 경로");
    expectString(item.integrity, "읽기 도구 무결성 상태");
  }
  expectString(record.mode, "읽기 도구 실행 모드");
  expectString(record.engineDir, "읽기 도구 폴더");
  return value as EngineRegistry;
}

export function validateCodeInventory(value: unknown): CodeInventory {
  const record = expectRecord(value, "코드 분석 결과");
  expectString(record.project, "코드 프로젝트 이름");
  for (const key of codeInventoryCollections) {
    const items = expectArray(record[key], `코드 ${key}`);
    items.forEach((item) => validateCodeItem(item, `코드 ${key}`));
  }
  expectRecord(record.summary, "코드 분석 요약");
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
  expectArray(record.tables, "DB 테이블 목록").forEach((table) => validateDbTable(table));
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
  expectArray(record.nodes, "시각화 항목").forEach((node) => {
    const item = expectRecord(node, "시각화 항목");
    expectString(item.id, "시각화 항목 ID");
    expectString(item.kind, "시각화 항목 종류");
    expectString(item.title, "시각화 항목 이름");
  });
  expectArray(record.edges, "시각화 관계").forEach((edge) => {
    const item = expectRecord(edge, "시각화 관계");
    expectString(item.id, "시각화 관계 ID");
    expectString(item.from, "시각화 관계 시작 대상");
    expectString(item.to, "시각화 관계 도착 대상");
    expectString(item.kind, "시각화 관계 종류");
  });
  expectArray(record.warnings, "시각화 경고");
  return value as VisualMap;
}

export function validateInventorySearchResult(value: unknown): InventorySearchResult {
  const record = expectRecord(value, "검색 응답");
  expectArray(record.hits, "검색 결과");
  if (typeof record.total !== "number" || typeof record.truncated !== "boolean") {
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
  if (!Array.isArray(record.dbProfiles)) {
    throw contractError("프로젝트 DB 연결 목록 형식이 올바르지 않습니다");
  }
  return value as Workspace;
}

export function validateWorkspaceList(value: unknown): Workspace[] {
  if (!Array.isArray(value)) {
    throw contractError("프로젝트 목록 형식이 올바르지 않습니다");
  }
  value.forEach(validateWorkspace);
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

function validateCodeItem(value: unknown, label: string): void {
  const item = expectRecord(value, label);
  expectString(item.id, `${label} ID`);
  expectString(item.name, `${label} 이름`);
  expectString(item.kind, `${label} 종류`);
}

function validateDbTable(value: unknown): void {
  const table = expectRecord(value, "DB 테이블");
  expectString(table.name, "DB 테이블 이름");
  expectArray(table.columns, "DB 컬럼 목록").forEach((column) => {
    const item = expectRecord(column, "DB 컬럼");
    expectString(item.name, "DB 컬럼 이름");
  });
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

function contractError(message: string): Error {
  return new Error(`분석 결과 계약 오류: ${message}`);
}
