import { Code2, Database } from "lucide-react";
import type { ReactNode } from "react";
import type { EngineRegistry } from "../../types/engine";
import { codeInventoryCodeItems, codeInventoryItemCount, dbProfileSourceLabel } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import type { VisualMap } from "../../types/visual-map";
import { EngineStatus } from "../common/EngineStatus";
import { DiagnosticsExport } from "../common/DiagnosticsExport";

export function WorkbenchStatusBar({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
}) {
  const codeSourceSummary = codeSourceStatusSummary(workspaceControls.codeInventory);
  const hasCodeInventory = Boolean(workspaceControls.codeInventory);
  const hasCodeItems = codeInventoryItemCount(workspaceControls.codeInventory) > 0;
  const hasDbInventory = Boolean(dbProfileControls.inventory);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const workspaceRequiredLabel = workspaceControls.repoSourceMode === "github" ? "GitHub URL 필요" : "로컬 폴더 필요";
  const workspaceLabel =
    workspaceControls.currentWorkspace?.name ??
    (workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "저장소 복제 준비"
        : "프로젝트 열기 준비"
      : workspaceRequiredLabel);
  const mapSummary = mapStatusSummary(visualMapControls.currentMap);
  const evidenceSummary = evidenceStatusSummary(visualMapControls.currentMap);

  return (
    <footer className="statusbar">
      <span className="status-workspace">프로젝트: {workspaceLabel}</span>
      <EngineStatus
        label="코드 읽기"
        role="code"
        registry={engineRegistry}
        error={engineError}
        missingText={hasCodeInventory ? "저장된 목록" : undefined}
        missingTitle={hasCodeInventory ? "저장된 코드 목록으로 보는 중입니다. 다시 읽으려면 코드 읽기 도구가 필요합니다." : undefined}
      />
      <EngineStatus
        label="DB 읽기"
        role="db"
        registry={engineRegistry}
        error={engineError}
        missingText={hasDbInventory ? "저장된 구조" : undefined}
        missingTitle={hasDbInventory ? "저장된 DB 구조로 보는 중입니다. 다시 읽으려면 DB 읽기 도구가 필요합니다." : undefined}
      />
      {hasWorkspace && (
        <>
          <span className="push status-source">
            <Database size={12} /> DB 연결:{" "}
            {dbSourceStatusSummary(dbProfileControls, hasCodeItems)}
          </span>
          <span className="status-map" title={mapSummary.title}>캔버스: {mapSummary.text}</span>
          <span className={`status-quality ${evidenceSummary.tone}`} title={evidenceSummary.title}>
            근거: {evidenceSummary.text}
          </span>
          <span className="status-source">
            <Code2 size={12} /> 코드 위치: {codeSourceSummary}
          </span>
          <span className="status-snapshot">읽은 시간: {formatSnapshotTime(visualMapControls.snapshotSavedAt)}</span>
        </>
      )}
      {workspaceControls.operationStatus.phase !== "idle" && (
        <details className={`operation-details ${workspaceControls.operationStatus.phase}`}>
          <summary>{workspaceControls.operationStatus.message}</summary>
          {workspaceControls.operationStatus.details && <pre>{workspaceControls.operationStatus.details}</pre>}
        </details>
      )}
      {hasWorkspace && (
        <DiagnosticsExport
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
          engineRegistry={engineRegistry}
        />
      )}
      {devSlot}
    </footer>
  );
}

function dbSourceStatusSummary(dbProfileControls: DbProfileControls, hasCodeItems: boolean): string {
  const tables = dbProfileControls.inventory?.tables ?? [];
  const tableCount = tables.length;
  const columnCount = tables.reduce((sum, table) => sum + table.columns.length, 0);
  const missingColumnTables = tables.filter((table) => table.columns.length === 0).length;
  const source = dbProfileControls.activeProfile
    ? dbProfileSourceLabel(dbProfileControls.activeProfile.source)
    : dbProfileControls.inventory
      ? "저장된 구조"
      : hasCodeItems
        ? "영향 범위 대기"
        : "연결 전";

  if (!dbProfileControls.inventory) {
    return source;
  }
  if (tableCount === 0) {
    return `${source} · 테이블 없음`;
  }
  if (missingColumnTables > 0 && columnCount > 0) {
    return `${source} · 컬럼 일부 대기 ${missingColumnTables}/${tableCount} · 컬럼 ${columnCount}`;
  }
  return columnCount > 0 ? `${source} · 테이블 ${tableCount} · 컬럼 ${columnCount}` : `${source} · 컬럼 대기 · 테이블 ${tableCount}`;
}

function codeSourceStatusSummary(inventory: CodeInventory | null): string {
  if (!inventory) {
    return "코드 읽기 전";
  }
  const codeCount = codeInventoryCodeItems(inventory).length;
  const parts = [
    inventory.routes.length > 0 ? `API ${inventory.routes.length}` : null,
    codeCount > 0 ? `코드 ${codeCount}` : null,
    inventory.files.length > 0 ? `파일 ${inventory.files.length}` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "비어 있음";
}

function formatSnapshotTime(value: string | null): string {
  if (!value) {
    return "아직 안 읽음";
  }
  const timestamp = Number(value);
  const date = Number.isFinite(timestamp) ? new Date(value.length <= 10 ? timestamp * 1000 : timestamp) : new Date(value);
  return Number.isNaN(date.getTime())
    ? "확인 필요"
    : new Intl.DateTimeFormat("ko-KR", { dateStyle: "short", timeStyle: "short" }).format(date);
}

function mapStatusSummary(map: VisualMap | null): { text: string; title: string } {
  if (!map) {
    return { text: "코드/DB 연결", title: "코드 또는 DB를 연결하면 캔버스가 표시됩니다." };
  }
  const warningText = map.warnings.length > 0 ? ` · 경고 ${map.warnings.length}` : "";
  if (map.nodes.length === 0) {
    return { text: `대상 대기${warningText}`, title: `${modeLabel(map.mode)} · 선택할 항목이 아직 없습니다.${warningText}` };
  }
  if (map.edges.length === 0) {
    return { text: `답 기준 선택 · 항목 ${map.nodes.length}${warningText}`, title: `${modeLabel(map.mode)} · 항목 ${map.nodes.length}개 · 카드를 선택해 답 기준을 좁히세요.${warningText}` };
  }
  return {
    text: `항목 ${map.nodes.length} · 관계 ${map.edges.length}${warningText}`,
    title: `${modeLabel(map.mode)} · 항목 ${map.nodes.length}개 · 관계 ${map.edges.length}개${warningText}`,
  };
}

function evidenceStatusSummary(map: VisualMap | null): { text: string; title: string; tone: "ready" | "candidate" | "empty" } {
  if (!map) {
    return { text: "연결 대기", title: "코드 또는 DB를 읽으면 근거가 표시됩니다.", tone: "empty" };
  }
  const edges = map.edges;
  if (edges.length === 0) {
    return map.nodes.length > 0
      ? { text: "답 기준 선택", title: `항목 ${map.nodes.length}개 · 카드를 선택하면 파일/컬럼 근거가 표시됩니다.`, tone: "empty" }
      : { text: "대상 대기", title: "코드 또는 DB 항목을 읽으면 선택할 대상이 표시됩니다.", tone: "empty" };
  }
  const confirmed = edges.filter((edge) => !edge.kind.startsWith("candidate") && edge.kind !== "code_flow" && edge.evidence.length > 0).length;
  const candidate = edges.filter((edge) => edge.kind.startsWith("candidate")).length;
  const inferred = edges.filter((edge) => edge.kind === "code_flow").length;
  const typed = edges.length - confirmed - candidate - inferred;
  const text = [
    confirmed > 0 ? `직접 ${confirmed}` : null,
    typed > 0 ? `구조 ${typed}` : null,
    candidate > 0 ? `후보 ${candidate}` : null,
    inferred > 0 ? `이름 단서 ${inferred}` : null,
  ].filter(Boolean).join(" · ");
  return {
    text,
    title: `직접 근거 ${confirmed}개 · 구조 ${typed}개 · 후보 ${candidate}개 · 이름 단서 ${inferred}개 · 직접/구조 우선`,
    tone: candidate > 0 || inferred > 0 ? "candidate" : confirmed > 0 || typed > 0 ? "ready" : "empty",
  };
}

function modeLabel(mode: VisualMap["mode"]): string {
  if (mode === "api-flow") return "API가 닿는 코드";
  if (mode === "table-usage") return "테이블 연결";
  if (mode === "column-impact") return "컬럼 변경 범위";
  if (mode === "search-focus") return "대상 주변 근거";
  return "전체 구조";
}
