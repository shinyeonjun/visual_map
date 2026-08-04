import { Braces, Code2, Database, GitBranch, ShieldCheck, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import type { EngineRegistry } from "../../types/engine";
import type { CodeEvidenceCollector, CodeEvidenceSummary } from "../../types/workspace";
import { codeInventoryRouteCount, codeInventorySymbolCount, dbInventoryTableCount } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { visualEdgeTruthClass } from "../../visual/labels";
import { EngineStatus } from "../../components/common/EngineStatus";
import { DiagnosticsExport } from "../../components/common/DiagnosticsExport";

export function MapStatusBar({
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
  const hasCodeInventory = Boolean(workspaceControls.codeInventory);
  const hasDbInventory = Boolean(dbProfileControls.inventory);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const codeCount = codeInventorySymbolCount(workspaceControls.codeInventory);
  const routeCount = codeInventoryRouteCount(workspaceControls.codeInventory);
  const tableCount = dbInventoryTableCount(dbProfileControls.inventory);
  const edges = visualMapControls.currentMap?.edges ?? [];
  const confirmedCount = edges.filter((edge) => visualEdgeTruthClass(edge) === "confirmed").length;
  const candidateCount = edges.filter((edge) => visualEdgeTruthClass(edge) === "candidate").length;
  const evidence = workspaceControls.codeInventory?.evidence ?? null;

  // One line, few items, tabular numbers. Everything that used to crowd this
  // bar has a better home now: coverage in the banner, position in the
  // breadcrumb, evidence detail in the inspector.
  return (
    <footer className="statusbar">
      {hasWorkspace && (
        <div className="status-metrics" aria-label="분석 요약">
          <span>
            <Braces size={13} /> API <b>{routeCount.toLocaleString("ko-KR")}</b>
          </span>
          <span>
            <Code2 size={13} /> 코드 <b>{codeCount.toLocaleString("ko-KR")}</b>
          </span>
          <span>
            <Database size={13} /> 테이블 <b>{tableCount.toLocaleString("ko-KR")}</b>
          </span>
        </div>
      )}
      {hasWorkspace && edges.length > 0 ? (
        <div className="status-truth" aria-label="현재 지도 관계 요약">
          <span className="confirmed">
            <GitBranch size={12} /> 확정 연결 <b>{confirmedCount.toLocaleString("ko-KR")}</b>
          </span>
          <span className="candidate">
            후보 연결 <b>{candidateCount.toLocaleString("ko-KR")}</b>
          </span>
        </div>
      ) : null}
      {evidence ? <ProjectEvidence evidence={evidence} /> : null}
      {/*
        Engine health belongs on the bar, not behind a disclosure: a reader who
        cannot see that an engine is down has no way to read a thin map right.
      */}
      <div className="status-engines push" aria-label="엔진 상태">
        <EngineStatus
          label="codebase-memory"
          role="code"
          registry={engineRegistry}
          error={engineError}
          missingText={hasCodeInventory ? "저장된 목록" : undefined}
          missingTitle={
            hasCodeInventory
              ? "저장된 코드 목록으로 보는 중입니다. 다시 읽으려면 코드 읽기 도구가 필요합니다."
              : undefined
          }
        />
        <EngineStatus
          label="database-memory"
          role="db"
          registry={engineRegistry}
          error={engineError}
          missingText={hasDbInventory ? "저장된 구조" : undefined}
          missingTitle={
            hasDbInventory ? "저장된 DB 구조로 보는 중입니다. 다시 읽으려면 DB 읽기 도구가 필요합니다." : undefined
          }
        />
      </div>
      {hasWorkspace && (
        <span className="status-snapshot">스냅샷 {formatSnapshotTime(visualMapControls.snapshotSavedAt)}</span>
      )}
      {workspaceControls.operationStatus.phase !== "idle" && (
        <details className={`operation-details ${workspaceControls.operationStatus.phase}`}>
          <summary title={workspaceControls.operationStatus.message}>
            {workspaceControls.operationStatus.message}
          </summary>
          {workspaceControls.operationStatus.details && <pre>{workspaceControls.operationStatus.details}</pre>}
        </details>
      )}
      <details className="status-diagnostics">
        <summary>진단</summary>
        <div className="status-diagnostics-body">
          {hasWorkspace ? (
            <DiagnosticsExport
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
              engineRegistry={engineRegistry}
            />
          ) : null}
        </div>
      </details>
      {devSlot}
    </footer>
  );
}

const EVIDENCE_LABELS: Record<string, string> = {
  "build-graph": "빌드 구조",
  "ci-evidence": "테스트·검증",
  frameworks: "프레임워크",
  contracts: "API 계약",
  "database-assets": "ORM·마이그레이션",
  deployment: "배포",
  messaging: "메시징",
  "git-revision": "Git 변경",
  telemetry: "런타임 추적",
};

function ProjectEvidence({ evidence }: { evidence: CodeEvidenceSummary }) {
  const observed = evidence.collectors.filter((collector) =>
    ["collected", "partial"].includes(collector.status),
  ).length;

  return (
    <details className="status-evidence">
      <summary title="코드 심볼 외에 수집된 프로젝트 근거 보기">
        <ShieldCheck size={12} />
        <span>프로젝트 근거</span>
        <b>
          {observed}/{evidence.collectors.length}
        </b>
        <span className="evidence-segments" aria-hidden="true">
          {evidence.collectors.map((collector) => (
            <i key={collector.id} data-status={collector.status} />
          ))}
        </span>
      </summary>
      <section className="evidence-board" aria-label="프로젝트 근거 수집 상태">
        <header>
          <div>
            <strong>프로젝트 근거</strong>
            <small>코드 심볼 밖의 구조·계약·검증 신호입니다. 미감지는 오류가 아닙니다.</small>
          </div>
          <dl>
            <div>
              <dt>사실</dt>
              <dd>{evidence.factCount.toLocaleString("ko-KR")}</dd>
            </div>
            <div>
              <dt>관계</dt>
              <dd>{evidence.relationCount.toLocaleString("ko-KR")}</dd>
            </div>
            <div>
              <dt>진단</dt>
              <dd>{evidence.diagnosticCount.toLocaleString("ko-KR")}</dd>
            </div>
          </dl>
        </header>
        <ul className="evidence-provider-grid">
          {evidence.collectors.map((collector) => (
            <EvidenceProvider key={collector.id} collector={collector} />
          ))}
        </ul>
        {evidence.diagnostics.length > 0 ? (
          <details className="evidence-diagnostics">
            <summary>
              <TriangleAlert size={12} /> 수집 진단 {evidence.diagnosticCount.toLocaleString("ko-KR")}개
            </summary>
            <ul>
              {evidence.diagnostics.map((diagnostic, index) => (
                <li key={`${diagnostic.collector}:${diagnostic.code}:${diagnostic.path ?? index}`}>
                  <b>{EVIDENCE_LABELS[diagnostic.collector] ?? diagnostic.collector}</b>
                  <span>{diagnostic.message}</span>
                  {diagnostic.path ? <code>{diagnostic.path}</code> : null}
                </li>
              ))}
              {evidence.diagnosticsHidden > 0 ? <li>외 {evidence.diagnosticsHidden}개</li> : null}
            </ul>
          </details>
        ) : null}
      </section>
    </details>
  );
}

function EvidenceProvider({ collector }: { collector: CodeEvidenceCollector }) {
  const status = evidenceStatus(collector.status);
  return (
    <li data-status={collector.status}>
      <div>
        <strong>{EVIDENCE_LABELS[collector.id] ?? collector.capability}</strong>
        <em>{status}</em>
      </div>
      <small>
        사실 {collector.factCount.toLocaleString("ko-KR")} · 관계 {collector.relationCount.toLocaleString("ko-KR")}
      </small>
      {collector.detectedBy.length > 0 ? (
        <code title={collector.detectedBy.join("\n")}>{collector.detectedBy[0]}</code>
      ) : collector.tool ? (
        <code>{collector.toolVersion ? `${collector.tool} ${collector.toolVersion}` : collector.tool}</code>
      ) : (
        <code>해당 파일이나 실행 근거 없음</code>
      )}
    </li>
  );
}

function evidenceStatus(status: string): string {
  switch (status) {
    case "collected":
      return "근거 있음";
    case "partial":
      return "일부 근거";
    case "unavailable":
      return "도구 없음";
    case "failed":
      return "확인 필요";
    default:
      return "미감지";
  }
}

function formatSnapshotTime(value: string | null): string {
  if (!value) {
    return "아직 안 읽음";
  }
  const timestamp = Number(value);
  const date = Number.isFinite(timestamp)
    ? new Date(value.length <= 10 ? timestamp * 1000 : timestamp)
    : new Date(value);
  return Number.isNaN(date.getTime())
    ? "확인 필요"
    : new Intl.DateTimeFormat("ko-KR", { dateStyle: "short", timeStyle: "short" }).format(date);
}
