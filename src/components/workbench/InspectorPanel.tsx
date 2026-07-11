import { invoke } from "@tauri-apps/api/core";
import { CheckSquare, Copy, ExternalLink, FileText, FolderOpen, Info, MousePointer2, Plus, Trash2, Type } from "lucide-react";
import { useState } from "react";
import {
  confidenceBadgeTone,
  confidenceLabel,
  confidenceReason as confidenceReasonLabel,
  normalizeConfidence,
} from "../../visual/confidence";
import { dbProfileWorkStarted } from "../../types/controls";
import { codeInventoryItemCount, dbInventoryTableKey, dbProfileSourceLabel } from "../../types/workspace";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory, CodeInventoryItem, DbInventoryColumn, DbInventoryTable } from "../../types/workspace";
import type { ApiReadingAnswer, ImpactReviewItem, VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { copyValue } from "../common/copyValue";
import { focusDbProfileSetup as focusDbProfileInput } from "../common/focusSourceSetup";
import { focusGlobalSearch } from "../common/focusGlobalSearch";

type InspectorAnswer = {
  kicker: string;
  title: string;
  sentence: string;
  tone: "confirmed" | "candidate" | "neutral";
  metrics: Array<{ label: string; value: string; tone?: "green" | "amber" | "gray" }>;
  steps: string[];
  note?: string;
};

type EdgeCounts = {
  confirmed: number;
  typed: number;
  inferred: number;
  candidate: number;
};

type InspectorAction = {
  label: string;
  run: () => void;
  primary?: boolean;
  disabled?: boolean;
};

type InvestigationItem = {
  path: string;
  line: number;
  column: number;
  evidenceId: string;
  checked: boolean;
};

const INVESTIGATION_STORAGE_PREFIX = "backend-visual-map:investigation:v1:";
const INVESTIGATION_LIMIT = 50;

export function InspectorPanel({
  showDbSetup,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  showDbSetup?: () => void;
  showWorkspaceSetup?: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const selectedNode = visualMapControls.selectedNode;
  const selectedEdge = visualMapControls.selectedEdge;
  const apiReading = visualMapControls.mode === "api-flow" ? visualMapControls.currentMap?.apiReading ?? null : null;
  const selectedEdgeFrom = selectedEdge ? endpointLabel(selectedEdge.from, visualMapControls.currentMap) : null;
  const selectedEdgeTo = selectedEdge ? endpointLabel(selectedEdge.to, visualMapControls.currentMap) : null;
  const columnImpact = selectedNode?.kind === "column" ? columnImpactSummary(selectedNode, visualMapControls.currentMap) : null;
  const selectedNodeHasCodeRelation = selectedNode ? nodeHasCodeRelation(selectedNode, visualMapControls.currentMap) : false;
  const nodeEvidence = selectedNode ? nodeEvidenceSummary(selectedNode, visualMapControls.currentMap) : null;
  const focusedNodeId = selectedNode?.id ?? visualMapControls.currentMap?.focus ?? "";
  const focusedCodeId = selectedNode?.source === "code"
    ? selectedNode.id.replace(/^code:/, "")
    : !selectedNode && focusedNodeId.startsWith("code:")
      ? focusedNodeId.replace(/^code:/, "")
      : null;
  const selectedCode = focusedNodeId.startsWith("db:")
    ? null
    : codeInventoryItemById(workspaceControls.codeInventory, focusedCodeId) ?? workspaceControls.selectedCodeItem;
  const dbTables = dbProfileControls.inventory?.tables ?? [];
  const dbMissingColumnTables = dbTables.filter((table) => table.columns.length === 0).length;
  const dbNeedsColumns = dbTables.length > 0 && !dbTables.some((table) => table.columns.length > 0);
  const dbSetupAction = showDbSetup ?? (() => focusDbProfileInput(dbProfileControls));
  const dbColumnAction = dbNeedsColumns
    ? {
        label:
          !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
            ? "DB 연결 저장"
            : dbProfileControls.canIndexProfile
              ? "다시 읽기"
              : "DB 정보 입력",
        run:
          !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
            ? dbProfileControls.saveProfile
            : dbProfileControls.canIndexProfile
              ? dbProfileControls.indexProfile
              : dbSetupAction,
        primary: true,
        disabled: dbProfileControls.busy,
      }
    : null;
  const focusedColumn = selectedNode?.kind === "column"
    ? columnRefFromNodeId(selectedNode.id)
    : !selectedNode
      ? columnRefFromNodeId(visualMapControls.currentMap?.focus ?? "")
      : null;
  const selectedNodeTableKey = selectedNode ? tableKeyFromDbNodeId(selectedNode.id) : focusedColumn?.tableKey ?? null;
  const useSelectedTable =
    focusedNodeId.startsWith("db:") ||
    visualMapControls.mode === "table-usage" ||
    visualMapControls.mode === "column-impact";
  const selectedTable =
    dbTables.find((table) => dbInventoryTableKey(table) === selectedNodeTableKey) ??
    (!useSelectedTable || focusedNodeId.startsWith("code:")
      ? null
      : dbTables.find((table) => dbInventoryTableKey(table) === dbProfileControls.selectedTableKey)) ??
    null;
  const selectedColumn =
    focusedColumn && selectedTable && dbInventoryTableKey(selectedTable) === focusedColumn.tableKey
      ? selectedTable.columns.find((column) => column.name === focusedColumn.columnName) ?? null
      : null;
  const tableColumnAction =
    !selectedEdge && !selectedCode && !selectedColumn && selectedTable && (!selectedNode || selectedNode.kind === "table")
      ? firstTableColumnAction(selectedTable, dbProfileControls, visualMapControls)
      : null;
  const answer = apiReading ? apiReadingSummary(apiReading) : inspectorAnswer({
    edge: selectedEdge,
    node: selectedNode,
    code: selectedCode,
    table: selectedTable,
    column: selectedColumn,
    map: visualMapControls.currentMap,
    dbNeedsColumns,
    dbMissingColumnTables,
    dbTableCount: dbTables.length,
    codeItemCount: codeInventoryItemCount(workspaceControls.codeInventory),
    hasWorkspace: Boolean(workspaceControls.currentWorkspace),
    needsGithub: workspaceControls.repoSourceMode === "github",
  });
  const hasSelection = Boolean(selectedEdge || selectedNode || selectedCode || selectedColumn || selectedTable);
  const emptyAction = hasSelection
    ? null
    : inspectorEmptyAction(workspaceControls, dbProfileControls, visualMapControls, dbSetupAction);
  const firstNodeRelation = selectedNode ? firstNodeRelationEdge(selectedNode, visualMapControls.currentMap) : null;
  const selectedEdgeAction = selectedEdge
    ? edgeEndpointAction(selectedEdge, visualMapControls.currentMap, visualMapControls.selectNode)
    : null;
  const selectionAction =
    selectedEdgeAction
      ? selectedEdgeAction
      : (selectedTable?.columns.length === 0 || (selectedNode?.kind === "table" && dbNeedsColumns)) && dbColumnAction
      ? dbColumnAction
      : tableColumnAction
      ? tableColumnAction
      : !selectedEdge && firstNodeRelation
      ? { label: "근거 보기", run: () => visualMapControls.selectEdge(firstNodeRelation), primary: true }
      : null;

  return (
    <section className="side-card inspector">
      <div className="panel-header">
        <Info size={16} />
        <h2>답 패널</h2>
      </div>
      <AnswerSummary answer={answer} action={selectionAction ?? emptyAction} />
      {apiReading ? (
        <ApiReadingInspector
          answer={apiReading}
          map={visualMapControls.currentMap}
          onSelectNode={visualMapControls.selectNode}
          onSelectEdge={visualMapControls.selectEdge}
        />
      ) : null}
      {hasSelection && (
        <>
          <label className="field-label">
            {selectedEdge
              ? "선택한 관계"
              : selectedColumn
                ? "선택된 컬럼"
                : selectedNode
                  ? "선택된 항목"
                  : selectedCode
                    ? "선택된 코드"
                    : "선택된 테이블"}
          </label>
          {selectedEdge ? (
            <>
              <div className="edge-summary">
                <code title={selectedEdge.from}>{selectedEdgeFrom}</code>
                <span>&rarr;</span>
                <code title={selectedEdge.to}>{selectedEdgeTo}</code>
              </div>
              <div className="kv">
                <span>유형</span>
                <strong>{edgeKindLabel(selectedEdge)}</strong>
                <span>판정</span>
                <strong>{relationshipSourceLabel(selectedEdge)}</strong>
                <span>근거 수준</span>
                <strong className={`badge ${edgeTrustTone(selectedEdge)}`}>{edgeTrustLabel(selectedEdge)}</strong>
                <span>설명</span>
                <strong>{relationshipReason(selectedEdge)}</strong>
                <span>근거 기준</span>
                <strong>{edgeTrustReason(selectedEdge)}</strong>
              </div>
              <label className="field-label">근거</label>
              <div className="chips">
                {selectedEdge.evidence.length > 0 ? (
                  selectedEdge.evidence.map((item) => (
                    <span className={edgeEvidenceTone(selectedEdge)} key={`${item.kind}-${item.text}`} title={`${item.kind}: ${item.text}`}>
                      {item.kind}: {item.text}
                    </span>
                  ))
                ) : (
                  <span className="neutral">{emptyEvidenceLabel(selectedEdge)}</span>
                )}
              </div>
              <CopyRow values={[["관계", edgeCopySummary(selectedEdge, visualMapControls.currentMap)], ["기준", selectedEdge.from], ["연결 대상", selectedEdge.to]]} />
              <EdgeTargetRow edge={selectedEdge} map={visualMapControls.currentMap} onSelect={visualMapControls.selectNode} />
            </>
          ) : selectedColumn && selectedTable ? (
            <>
              <div className="edge-summary">
                <code>{`${dbInventoryTableKey(selectedTable)}.${selectedColumn.name}`}</code>
              </div>
              <div className="kv">
                <span>타입</span>
                <strong>{selectedColumn.dataType ?? "-"}</strong>
                <span>PK</span>
                <strong>{selectedColumn.isPrimaryKey ? "예" : "아니오"}</strong>
                <span>FK</span>
                <strong>{selectedColumn.isForeignKey ? "예" : "아니오"}</strong>
                <span>NULL</span>
                <strong>{selectedColumn.nullable === null || selectedColumn.nullable === undefined ? "-" : selectedColumn.nullable ? "허용" : "불가"}</strong>
              </div>
              <CopyRow
                values={[
                  ["컬럼", `${dbInventoryTableKey(selectedTable)}.${selectedColumn.name}`],
                  ["타입", selectedColumn.dataType ?? ""],
                ]}
              />
              {columnImpact && (
                <>
                  <label className="field-label">{selectedNodeHasCodeRelation ? "영향 요약" : "관계 요약"}</label>
                  <div className="kv">
                    <span>직접 근거</span>
                    <strong>{columnImpact.directCount}개</strong>
                    <span>후보 근거</span>
                    <strong className={`badge ${columnImpact.candidateCount ? "amber" : "gray"}`}>
                      {columnImpact.candidateCount}개
                    </strong>
                    <span>제약</span>
                    <strong>{columnImpact.constraints}</strong>
                  </div>
                  <span className="secret-note">
                    후보 근거는 이름 기반이며 직접 증거가 아닙니다.
                  </span>
                </>
              )}
            </>
          ) : selectedNode ? (
            <>
              <div className="edge-summary">
                <code>{nodeDisplayTitle(selectedNode)}</code>
              </div>
              <div className="kv">
                <span>종류</span>
                <strong>{selectedNode.kind}</strong>
                <span>출처</span>
                <strong>{nodeSourceLabel(selectedNode.source)}</strong>
                <span>근거 수준</span>
                <strong className={`badge ${nodeEvidence?.badgeTone ?? "gray"}`}>{nodeEvidence?.confidence ?? "-"}</strong>
                <span>연결</span>
                <strong>{nodeEvidence?.connectionSummary ?? "-"}</strong>
              </div>
              {nodeEvidence && (
                <>
                  <label className="field-label">근거</label>
                  <div className="chips">
                    {nodeEvidence.evidence.map((item) => (
                      <span className={item.tone} key={item.key} title={item.text}>
                        {item.text}
                      </span>
                    ))}
                  </div>
                  {nodeEvidence.relatedFiles.length > 0 && (
                    <>
                      <label className="field-label">관련 파일</label>
                      <div className="files">
                        {nodeEvidence.relatedFiles.map((file) => (
                          <span key={file}>
                            <FileText size={13} />
                            {file}
                          </span>
                        ))}
                      </div>
                    </>
                  )}
                </>
              )}
              <CopyRow values={copyValuesForNode(selectedNode)} />
              {selectedNode.source === "code" && selectedCode && selectedNode.id === `code:${selectedCode.id}` && (
                <SourceJumpRow
                  key={`${workspaceControls.currentWorkspace?.id ?? "none"}:${selectedCode.id}`}
                  workspaceId={workspaceControls.currentWorkspace?.id ?? null}
                  code={selectedCode}
                />
              )}
              {columnImpact && (
                <>
                  <label className="field-label">{selectedNodeHasCodeRelation ? "영향 요약" : "관계 요약"}</label>
                  <div className="kv">
                    <span>직접 근거</span>
                    <strong>{columnImpact.directCount}개</strong>
                    <span>후보 근거</span>
                    <strong className={`badge ${columnImpact.candidateCount ? "amber" : "gray"}`}>
                      {columnImpact.candidateCount}개
                    </strong>
                    <span>제약</span>
                    <strong>{columnImpact.constraints}</strong>
                  </div>
                  <span className="secret-note">
                    후보 근거는 이름 기반이며 직접 증거가 아닙니다.
                  </span>
                </>
              )}
            </>
          ) : selectedCode ? (
            <>
              <div className="edge-summary">
                <code>{selectedCode.name}</code>
              </div>
              <div className="kv">
                <span>종류</span>
                <strong>{selectedCode.kind}</strong>
                <span>라인</span>
                <strong>{selectedCode.line ?? "-"}</strong>
                <span>경로</span>
                <strong title={selectedCode.filePath ?? undefined}>{compactPath(selectedCode.filePath) ?? "-"}</strong>
              </div>
              <CopyRow
                values={[
                  [
                    "위치",
                    selectedCode.filePath && selectedCode.line
                      ? `${selectedCode.filePath}:${selectedCode.line}${selectedCode.column ? `:${selectedCode.column}` : ""}`
                      : "",
                  ],
                  ["심볼", selectedCode.name],
                  ["경로", selectedCode.filePath ?? ""],
                  ["라인", selectedCode.line ? String(selectedCode.line) : ""],
                ]}
              />
              <SourceJumpRow
                key={`${workspaceControls.currentWorkspace?.id ?? "none"}:${selectedCode.id}`}
                workspaceId={workspaceControls.currentWorkspace?.id ?? null}
                code={selectedCode}
              />
            </>
          ) : selectedTable ? (
            <>
              <div className="edge-summary">
                <code>{selectedTable.schema ? `${selectedTable.schema}.${selectedTable.name}` : selectedTable.name}</code>
              </div>
              <div className="kv">
                <span>컬럼</span>
                <strong>{selectedTable.columns.length}</strong>
                <span>연결</span>
                <strong className="badge green">{dbProfileControls.activeProfile?.name ?? "활성"}</strong>
                <span>출처</span>
                <strong>{dbProfileControls.activeProfile ? dbProfileSourceLabel(dbProfileControls.activeProfile.source) : "-"}</strong>
              </div>
              <CopyRow
                values={[
                  ["테이블", selectedTable.name],
                  ["스키마", selectedTable.schema ?? ""],
                ]}
              />
              <label className="field-label">컬럼</label>
              <div className="files">
                {selectedTable.columns.slice(0, 8).map((column) => (
                  <span key={column.name}>
                    <Type size={13} />
                    {column.name}
                    <em>{column.dataType ?? (column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "")}</em>
                  </span>
                ))}
              </div>
            </>
          ) : null}
        </>
      )}
    </section>
  );
}

function apiReadingSummary(answer: ApiReadingAnswer): InspectorAnswer {
  return {
    kicker: "API 읽기 경로",
    title: answer.subject,
    sentence: answer.unknowns[0]?.kind === "handler-gap"
      ? answer.unknowns[0].detail
      : "확정 HANDLES/CALLS와 검증할 DB 후보를 분리해 표시합니다.",
    tone: "neutral",
    metrics: [
      { label: "읽기 단계", value: String(answer.steps.length), tone: "green" },
      { label: "DB 후보", value: String(answer.dbCandidates.length), tone: answer.dbCandidates.length > 0 ? "amber" : "gray" },
      { label: "확인 필요", value: String(answer.unknowns.length), tone: answer.unknowns.length > 0 ? "amber" : "gray" },
    ],
    steps: answer.steps.slice(0, 3).map((step) => `${step.rank}. ${step.title}${step.location ? ` · ${sourceLocationLabel(step.location)}` : ""}`),
    note: answer.truncationReason ? `표시 한도: ${answer.truncationReason}` : undefined,
  };
}

function ApiReadingInspector({
  answer,
  map,
  onSelectNode,
  onSelectEdge,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap | null;
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const confirmedEdges = map?.edges.filter((edge) => edge.kind === "code_handle" || edge.kind === "code_call") ?? [];
  const visibleConfirmedEdges = confirmedEdges.slice(0, 5);
  const hiddenConfirmedEdges = Math.max(0, confirmedEdges.length - visibleConfirmedEdges.length);
  return (
    <div className="api-reading-inspector" aria-label={`${answer.subject} API 읽기 답`}>
      <label className="field-label">이 순서로 읽기</label>
      <div className="api-reading-list">
        {answer.steps.map((step) => (
          <ApiInspectorItem item={step} map={map} onSelectNode={onSelectNode} meta={step.location ? sourceLocationLabel(step.location) : "소스 위치 없음"} key={step.id} />
        ))}
      </div>
      <label className="field-label">확정 HANDLES/CALLS 관계</label>
      <div className="api-reading-list">
        {visibleConfirmedEdges.length > 0 ? visibleConfirmedEdges.map((edge) => (
          <button type="button" onClick={() => onSelectEdge(edge)} key={`confirmed-${edge.id}`}>
            <span>
              <b>{edge.kind === "code_handle" ? "HANDLES" : "CALLS"}</b>{" "}
              {endpointLabel(edge.from, map)} &rarr; {endpointLabel(edge.to, map)}
            </span>
            <small title={edge.evidence[0]?.text ?? undefined}>
              {edge.evidence[0]?.text ?? "엔진이 읽은 확정 코드 관계"}
            </small>
          </button>
        )) : <span className="secret-note">확정 HANDLES/CALLS 관계가 없습니다.</span>}
        {hiddenConfirmedEdges > 0 ? <span className="secret-note">+{hiddenConfirmedEdges}개 확정 관계 더 있음</span> : null}
      </div>
      <ApiInspectorSection
        title="DB 후보"
        items={answer.dbCandidates}
        empty="현재 확정 CALLS 경로에서 테이블·쿼리 단서를 찾지 못했습니다. DB 미사용이 확정된 것은 아닙니다."
        map={map}
        onSelectNode={onSelectNode}
      />
      <ApiInspectorSection title="확인 안 된 구간" items={answer.unknowns} empty="현재 표시 범위에서 확인 안 된 구간이 없습니다." map={map} onSelectNode={onSelectNode} />
      <ApiInspectorSection title="권장 확인" items={answer.recommendedChecks} empty="추가 권장 확인이 없습니다." map={map} onSelectNode={onSelectNode} />
    </div>
  );
}

function ApiInspectorSection({
  title,
  items,
  empty,
  map,
  onSelectNode,
}: {
  title: string;
  items: ImpactReviewItem[];
  empty: string;
  map: VisualMap | null;
  onSelectNode: (node: VisualNode) => void;
}) {
  return (
    <>
      <label className="field-label">{title}</label>
      <div className="api-reading-list">
        {items.length > 0
          ? items.map((item) => <ApiInspectorItem item={item} map={map} onSelectNode={onSelectNode} meta={item.detail} key={item.id} />)
          : <span className="secret-note">{empty}</span>}
      </div>
    </>
  );
}

function ApiInspectorItem({
  item,
  meta,
  map,
  onSelectNode,
}: {
  item: ImpactReviewItem;
  meta: string;
  map: VisualMap | null;
  onSelectNode: (node: VisualNode) => void;
}) {
  const node = item.nodeId ? map?.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
  const content = (
    <>
      <span><b>#{item.rank}</b> {item.title}</span>
      <small title={meta}>{meta}</small>
    </>
  );
  return node ? <button type="button" onClick={() => onSelectNode(node)}>{content}</button> : <div>{content}</div>;
}

function sourceLocationLabel(location: { path: string; line?: number | null }): string {
  return `${location.path}${location.line ? `:${location.line}` : ""}`;
}

function AnswerSummary({ answer, action }: { answer: InspectorAnswer; action?: InspectorAction | null }) {
  const [firstStep, ...remainingSteps] = answer.steps;

  return (
    <div className={`answer-summary ${answer.tone}`}>
      <div className="answer-head">
        <span>{answer.kicker}</span>
        <strong title={answer.title}>{answer.title}</strong>
        <em className="answer-verdict">{answerVerdict(answer)}</em>
      </div>
      <p>{answer.sentence}</p>
      {firstStep && (
        <div className="answer-lead">
          <span>먼저 볼 것</span>
          <b>{firstStep}</b>
        </div>
      )}
      {action && (
        <div className="answer-next">
          <span>다음 행동</span>
          <button
            className={action.primary ? "primary-action compact answer-action" : "outline-action compact answer-action"}
            type="button"
            onClick={action.run}
            disabled={action.disabled}
          >
            <MousePointer2 size={12} />
            <span>{action.label}</span>
          </button>
        </div>
      )}
      {answer.metrics.length > 0 && (
        <div className="answer-metrics">
          {answer.metrics.map((metric) => (
            <span className={metric.tone} key={`${metric.label}-${metric.value}`}>
              <em>{metric.label}</em>
              <b>{metric.value}</b>
            </span>
          ))}
        </div>
      )}
      {remainingSteps.length > 0 && (
        <div className="answer-step-block">
          <span>그다음 확인</span>
          <ol className="answer-steps">
            {remainingSteps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        </div>
      )}
      {answer.note && <small>{answer.note}</small>}
    </div>
  );
}

function answerVerdict(answer: InspectorAnswer): string {
  if (answer.tone === "confirmed") {
    return "직접";
  }
  if (answer.tone === "candidate") {
    if (answer.metrics.some((metric) => metric.label === "컬럼" && metric.value === "0")) {
      return "보강";
    }
    return "후보";
  }
  if (answer.kicker === "시작" || answer.kicker === "다음 행동") {
    return "대기";
  }
  if (answer.title === "관계 없음" || answer.kicker === "코드/DB 목록") {
    return "관계 없음";
  }
  return "구조 근거";
}

function CopyRow({ values, label = "복사" }: { values: Array<[string, string]>; label?: string }) {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const visibleValues = values.filter(([, value]) => value.trim().length > 0);
  if (visibleValues.length === 0) {
    return null;
  }

  return (
    <div className="copy-row" aria-label={`${label} 도구`}>
      <span className="copy-row-title">{label}</span>
      {visibleValues.map(([label, value]) => {
        const key = `${label}-${value}`;
        return (
          <button
            type="button"
            key={key}
            onClick={() => {
              void copyValue(value).then((copied) => {
                if (!copied) {
                  return;
                }
                setCopiedKey(key);
                window.setTimeout(() => {
                  setCopiedKey((current) => (current === key ? null : current));
                }, 1200);
              });
            }}
          >
            <Copy size={12} />
            <span>{copiedKey === key ? "복사됨" : label}</span>
          </button>
        );
      })}
    </div>
  );
}

function SourceJumpRow({ workspaceId, code }: { workspaceId: string | null; code: CodeInventoryItem }) {
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [statusTone, setStatusTone] = useState<"success" | "error" | null>(null);
  const [investigationItems, setInvestigationItems] = useState<InvestigationItem[]>(() =>
    workspaceId ? loadInvestigation(workspaceId) : [],
  );
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  if (!workspaceId || !code.filePath) {
    return null;
  }
  const sourceWorkspaceId = workspaceId;

  const requestBase = {
    workspaceId,
    path: code.filePath,
  };
  const currentItem: InvestigationItem = {
    path: code.filePath,
    line: code.line ?? 1,
    column: code.column ?? 1,
    evidenceId: code.id,
    checked: false,
  };
  const currentKey = investigationKey(currentItem);
  const alreadyAdded = investigationItems.some((item) => investigationKey(item) === currentKey);

  function updateInvestigation(next: InvestigationItem[]) {
    const bounded = next.slice(-INVESTIGATION_LIMIT);
    setInvestigationItems(bounded);
    saveInvestigation(sourceWorkspaceId, bounded);
  }

  function addInvestigationItem() {
    if (alreadyAdded) {
      return;
    }
    updateInvestigation([...investigationItems, currentItem]);
  }

  function toggleInvestigationItem(key: string) {
    updateInvestigation(
      investigationItems.map((item) =>
        investigationKey(item) === key ? { ...item, checked: !item.checked } : item,
      ),
    );
  }

  function removeInvestigationItem(key: string) {
    updateInvestigation(investigationItems.filter((item) => investigationKey(item) !== key));
  }

  async function copyInvestigation() {
    const copied = await copyValue(investigationMarkdown(investigationItems));
    setCopyState(copied ? "copied" : "failed");
    window.setTimeout(() => setCopyState("idle"), 1200);
  }

  async function openEditor(editor: "vscode" | "cursor") {
    try {
      setBusyAction(editor);
      setStatus(null);
      setStatusTone(null);
      await invoke("open_source_location", {
        request: {
          ...requestBase,
          line: code.line ?? 1,
          column: code.column ?? 1,
          editor,
        },
      });
      setStatus(editor === "vscode" ? "VS Code에서 열었습니다" : "Cursor에서 열었습니다");
      setStatusTone("success");
    } catch (error) {
      setStatus(String(error));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function revealFile() {
    try {
      setBusyAction("reveal");
      setStatus(null);
      setStatusTone(null);
      await invoke("reveal_source_location", { request: requestBase });
      setStatus("파일 탐색기에서 표시했습니다");
      setStatusTone("success");
    } catch (error) {
      setStatus(String(error));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <>
      <div className="copy-row" aria-label="소스 위치 열기">
        <span className="copy-row-title">열기</span>
        <button type="button" data-source-action="vscode" disabled={busyAction !== null} onClick={() => void openEditor("vscode")}>
          <ExternalLink size={12} />
          <span>{busyAction === "vscode" ? "여는 중" : "VS Code"}</span>
        </button>
        <button type="button" data-source-action="cursor" disabled={busyAction !== null} onClick={() => void openEditor("cursor")}>
          <ExternalLink size={12} />
          <span>{busyAction === "cursor" ? "여는 중" : "Cursor"}</span>
        </button>
        <button type="button" data-source-action="reveal" disabled={busyAction !== null} onClick={() => void revealFile()}>
          <FolderOpen size={12} />
          <span>{busyAction === "reveal" ? "여는 중" : "탐색기"}</span>
        </button>
        <button
          type="button"
          data-investigation-action="add"
          disabled={alreadyAdded}
          onClick={addInvestigationItem}
        >
          <Plus size={12} />
          <span>{alreadyAdded ? "조사함에 있음" : "조사함 추가"}</span>
        </button>
        {status && <small role="status" data-source-status={statusTone}>{status}</small>}
      </div>
      {investigationItems.length > 0 && (
        <section className="investigation-tray" aria-label="로컬 조사함">
          <div className="investigation-tray-head">
            <span><CheckSquare size={13} />조사함 <b>{investigationItems.length}</b></span>
            <button
              type="button"
              data-investigation-action="copy"
              data-copy-state={copyState}
              onClick={() => void copyInvestigation()}
            >
              <Copy size={12} />
              {copyState === "copied" ? "복사됨" : copyState === "failed" ? "복사 실패" : "Markdown"}
            </button>
          </div>
          <div className="investigation-list">
            {investigationItems.map((item) => {
              const key = investigationKey(item);
              return (
                <div className={item.checked ? "investigation-item checked" : "investigation-item"} key={key}>
                  <button
                    type="button"
                    className="investigation-check"
                    data-investigation-action="toggle"
                    aria-label={`${sourceFileLabel(item.path)} 확인 ${item.checked ? "해제" : "완료"}`}
                    aria-pressed={item.checked}
                    onClick={() => toggleInvestigationItem(key)}
                  >
                    <span aria-hidden="true">{item.checked ? "✓" : ""}</span>
                  </button>
                  <span className="investigation-location" title={`${item.path}:${item.line}:${item.column}`}>
                    <b>{sourceFileLabel(item.path)}</b>
                    <small>{item.path}:{item.line}</small>
                    <code>{item.evidenceId}</code>
                  </span>
                  <button
                    type="button"
                    className="investigation-remove"
                    data-investigation-action="remove"
                    aria-label={`${sourceFileLabel(item.path)} 조사함에서 삭제`}
                    onClick={() => removeInvestigationItem(key)}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              );
            })}
          </div>
          <small className="investigation-privacy" data-investigation-storage="path-evidence-id-only">
            경로와 근거 ID만 이 PC에 저장됩니다.
          </small>
        </section>
      )}
    </>
  );
}

function investigationKey(item: InvestigationItem): string {
  return `${item.path}\u0000${item.line}\u0000${item.column}\u0000${item.evidenceId}`;
}

function sourceFileLabel(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? path;
}

function loadInvestigation(workspaceId: string): InvestigationItem[] {
  try {
    const value: unknown = JSON.parse(
      window.localStorage.getItem(`${INVESTIGATION_STORAGE_PREFIX}${workspaceId}`) ?? "[]",
    );
    if (!Array.isArray(value)) {
      return [];
    }
    return value.filter(isInvestigationItem).slice(-INVESTIGATION_LIMIT);
  } catch {
    return [];
  }
}

function saveInvestigation(workspaceId: string, items: InvestigationItem[]) {
  try {
    window.localStorage.setItem(`${INVESTIGATION_STORAGE_PREFIX}${workspaceId}`, JSON.stringify(items));
  } catch {
    // The tray remains usable for the current selection when local storage is unavailable.
  }
}

function isInvestigationItem(value: unknown): value is InvestigationItem {
  if (!value || typeof value !== "object") {
    return false;
  }
  const item = value as Partial<InvestigationItem>;
  return Boolean(
    typeof item.path === "string" && item.path.length > 0 && item.path.length <= 4096 &&
      Number.isInteger(item.line) && Number(item.line) > 0 && Number(item.line) <= 0xffff_ffff &&
      Number.isInteger(item.column) && Number(item.column) > 0 && Number(item.column) <= 0xffff_ffff &&
      typeof item.evidenceId === "string" && item.evidenceId.length > 0 && item.evidenceId.length <= 1024 &&
      typeof item.checked === "boolean",
  );
}

function investigationMarkdown(items: InvestigationItem[]): string {
  const lines = ["# Backend Visual Map 조사", ""];
  for (const item of items) {
    const location = `${item.path}:${item.line}:${item.column}`.replace(/`/g, "'");
    const evidenceId = item.evidenceId.replace(/`/g, "'");
    lines.push(`- [${item.checked ? "x" : " "}] \`${location}\` (근거: \`${evidenceId}\`)`);
  }
  return `${lines.join("\n")}\n`;
}

function codeInventoryItemById(inventory: CodeInventory | null, id: string | null): CodeInventoryItem | null {
  if (!inventory || !id) {
    return null;
  }
  for (const items of [
    inventory.routes,
    inventory.services,
    inventory.files,
    inventory.handlers,
    inventory.repositories,
    inventory.functions,
    inventory.classes,
    inventory.modules,
    inventory.unknown,
  ]) {
    const item = items.find((candidate) => candidate.id === id);
    if (item) {
      return item;
    }
  }
  return null;
}

function EdgeTargetRow({
  edge,
  map,
  onSelect,
}: {
  edge: VisualEdge;
  map: VisualMap | null;
  onSelect: (node: VisualNode) => void;
}) {
  const targets = [
    { label: "기준 보기", node: map?.nodes.find((item) => item.id === edge.from) ?? null },
    { label: "연결 대상 보기", node: map?.nodes.find((item) => item.id === edge.to) ?? null },
  ].filter((item): item is { label: string; node: VisualNode } => Boolean(item.node));

  if (targets.length === 0) {
    return null;
  }

  return (
    <div className="copy-row" aria-label="이동 도구">
      <span className="copy-row-title">이동</span>
      {targets.map(({ label, node }) => (
        <button type="button" key={`${label}-${node.id}`} title={nodeDisplayTitle(node)} onClick={() => onSelect(node)}>
          <MousePointer2 size={12} />
          <span>{label}</span>
        </button>
      ))}
    </div>
  );
}

function edgeEndpointAction(
  edge: VisualEdge,
  map: VisualMap | null,
  onSelect: (node: VisualNode) => void,
): InspectorAction | null {
  const toNode = map?.nodes.find((item) => item.id === edge.to) ?? null;
  const fromNode = map?.nodes.find((item) => item.id === edge.from) ?? null;
  const target = toNode ?? fromNode;
  if (!target) {
    return null;
  }
  return {
    label: toNode ? "연결 대상 보기" : "기준 보기",
    run: () => onSelect(target),
    primary: true,
  };
}

function inspectorEmptyAction(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
  showDbSetup: () => void,
): InspectorAction | null {
  if (!workspaceControls.currentWorkspace) {
    return null;
  }

  const firstVisibleNode = visualMapControls.currentMap?.nodes[0] ?? null;
  if (firstVisibleNode) {
    return {
      label: firstVisibleNodeActionLabel(firstVisibleNode),
      run: () => visualMapControls.selectNode(firstVisibleNode),
      primary: true,
    };
  }

  const dbStarted = dbProfileWorkStarted(dbProfileControls);

  if (!workspaceControls.codeInventory && !dbStarted) {
    const indexed = workspaceControls.codeStatus?.includes("완료") ?? false;
    if (!indexed && !workspaceControls.canIndexCode) {
      return {
        label: "DB 정보 입력",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return indexed
      ? { label: "코드 목록 열기", run: workspaceControls.loadCodeInventory, primary: true, disabled: workspaceControls.busy }
      : { label: "코드 읽기", run: workspaceControls.indexCodeRepository, primary: true, disabled: workspaceControls.busy };
  }

  if (!dbProfileControls.inventory) {
    if (!dbProfileControls.activeProfile) {
      return dbProfileControls.canSaveProfile
        ? { label: "DB 연결 저장", run: dbProfileControls.saveProfile, primary: true, disabled: dbProfileControls.busy }
        : { label: "DB 정보 입력", run: showDbSetup, disabled: dbProfileControls.busy };
    }
    const indexed = dbProfileControls.status?.includes("완료") ?? false;
    if (indexed) {
      return { label: "DB 목록 열기", run: dbProfileControls.loadInventory, primary: true, disabled: dbProfileControls.busy };
    }
    if (dbProfileControls.dbIndexBlockedReason) {
      return {
        label: "DB 설정 열기",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return dbProfileControls.canIndexProfile
      ? { label: "DB 읽기", run: dbProfileControls.indexProfile, primary: true, disabled: dbProfileControls.busy }
      : { label: "DB 정보 입력", run: showDbSetup, disabled: dbProfileControls.busy };
  }

  const tables = dbProfileControls.inventory.tables;
  if (tables.length > 0 && tables.some((table) => table.columns.length === 0)) {
    if (dbProfileControls.dbIndexBlockedReason) {
      return {
        label: "DB 설정 열기",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    if (!dbProfileControls.activeProfile && dbProfileControls.canSaveProfile) {
      return {
        label: "DB 연결 저장",
        run: dbProfileControls.saveProfile,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return dbProfileControls.canIndexProfile
      ? { label: "컬럼 보강", run: dbProfileControls.indexProfile, primary: true, disabled: dbProfileControls.busy }
      : { label: "DB 정보 입력", run: showDbSetup, primary: true, disabled: dbProfileControls.busy };
  }

  return hasSearchableInventory(workspaceControls, dbProfileControls)
    ? { label: "검색으로 대상 찾기", run: () => focusGlobalSearch(visualMapControls) }
    : null;
}

function firstVisibleNodeActionLabel(node: VisualNode): string {
  return `첫 ${nodeKindLabel(node.kind)} 보기`;
}

function hasSearchableInventory(workspaceControls: WorkspaceControls, dbProfileControls: DbProfileControls): boolean {
  return codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
}

function inspectorAnswer({
  edge,
  node,
  code,
  table,
  column,
  map,
  dbNeedsColumns,
  dbMissingColumnTables,
  dbTableCount,
  codeItemCount,
  hasWorkspace,
  needsGithub,
}: {
  edge: VisualEdge | null;
  node: VisualNode | null;
  code: CodeInventoryItem | null;
  table: DbInventoryTable | null;
  column: DbInventoryColumn | null;
  map: VisualMap | null;
  dbNeedsColumns: boolean;
  dbMissingColumnTables: number;
  dbTableCount: number;
  codeItemCount: number;
  hasWorkspace: boolean;
  needsGithub: boolean;
}): InspectorAnswer {
  if (edge) {
    const isCandidate = edge.kind.startsWith("candidate");
    const isInferred = edge.kind === "code_flow";
    const isStructural = isStructuralEdge(edge);
    const hasCodeEndpoint = edgeHasCodeEndpoint(edge);
    const hasEvidence = edge.evidence.length > 0;
    return {
      kicker: "관계 근거",
      title: `${endpointTitleLabel(edge.from, map)} → ${endpointTitleLabel(edge.to, map)}`,
      sentence: isCandidate
        ? !hasEvidence
          ? hasCodeEndpoint
            ? "후보 연결입니다. 변경 전 양끝 항목을 확인하세요."
            : "후보 연결입니다. 구조 판단 전 양끝 항목을 확인하세요."
          : hasCodeEndpoint
            ? "후보 연결입니다. 근거 확인 후 영향에 반영하세요."
            : "후보 연결입니다. 근거 확인 후 구조에 반영하세요."
        : isInferred
          ? "이름 단서 연결입니다. 실제 호출과 구분하세요."
          : isStructural
            ? "프로젝트 구조를 읽기 쉽게 묶은 관계입니다. 직접 호출이나 DB 제약과 구분하세요."
          : !hasEvidence
            ? hasCodeEndpoint
              ? "읽은 코드 구조의 관계입니다. 양끝 항목을 확인하세요."
              : "읽은 DB 구조/제약 관계입니다. 양끝 항목을 확인하세요."
          : hasCodeEndpoint
            ? "읽은 코드에서 확인된 1차 근거입니다."
            : "읽은 DB 구조에서 확인된 1차 근거입니다.",
      tone: isCandidate ? "candidate" : isInferred || isStructural || !hasEvidence ? "neutral" : "confirmed",
      metrics: [
        { label: "관계", value: edgeKindLabel(edge) },
        { label: "근거 수준", value: edgeTrustLabel(edge), tone: edgeTrustTone(edge) },
        { label: "근거 문장", value: hasEvidence ? `${edge.evidence.length}개` : "없음" },
      ],
      steps: [hasEvidence ? "근거 문장 확인" : "관계 구조와 출처 확인"],
      note: isCandidate ? "후보 근거는 이름 토큰 기반일 수 있어 직접 근거와 섞어 판단하면 안 됩니다." : undefined,
    };
  }

  if (table && column && (!node || node.kind === "column")) {
    return columnStructureAnswer(table, column);
  }

  if (node) {
    const counts = connectionCounts(map, node);
    if (node.kind === "group-domain") {
      return domainGroupAnswer(node, counts);
    }
    if (node.kind === "table" && dbNeedsColumns) {
      return {
        kicker: "테이블 근거",
        title: nodeDisplayTitle(node),
        sentence: "테이블 항목은 있지만 컬럼 구조가 없어 제약과 영향 범위를 아직 판단할 수 없습니다.",
        tone: "candidate",
        metrics: [
          { label: "테이블", value: String(dbTableCount), tone: "green" },
          { label: "컬럼", value: "0", tone: "amber" },
          { label: "PK/FK", value: "확인 불가", tone: "gray" },
        ],
        steps: ["DB 정보 입력 또는 다시 읽기", "컬럼 목록이 채워졌는지 확인", "영향/제약 답 다시 선택"],
        note: "테이블명만으로는 컬럼 변경 영향이나 FK 제약을 판단할 수 없습니다.",
      };
    }
    if (node.kind === "table" && table && table.columns.length > 0) {
      return tableStructureAnswer(table, connectionCounts(map, node));
    }
    const isCandidateHeavy = counts.candidate > 0 && counts.confirmed === 0;
    const isTypedOnly = counts.typed > 0 && counts.confirmed === 0 && counts.candidate === 0;
    const candidateNote = counts.candidate > 0 ? "후보 근거는 바뀔 수 있는 범위를 넓게 잡는 힌트입니다." : null;
    return {
      kicker: `${nodeKindLabel(node.kind)} 근거`,
      title: nodeDisplayTitle(node),
      sentence: nodeAnswerSentence(node, counts, map),
      tone: isCandidateHeavy ? "candidate" : isTypedOnly || node.source === "projection" ? "neutral" : "confirmed",
      metrics: nodeRelationMetrics(counts),
      steps: nodeAnswerSteps(node, map),
      note: candidateNote ?? undefined,
    };
  }

  if (code) {
    return {
      kicker: "코드 근거",
      title: code.name,
      sentence: "코드 목록에서 선택한 실제 항목입니다. 저장소 안의 파일/라인을 VS Code나 Cursor에서 바로 열 수 있습니다.",
      tone: "confirmed",
      metrics: [
        { label: "종류", value: code.kind },
        { label: "라인", value: code.line ? String(code.line) : "-" },
        { label: "경로", value: compactPath(code.filePath) ?? "-" },
      ],
      steps: codeAnswerSteps(code),
    };
  }

  if (table) {
    if (table.columns.length === 0) {
      return {
        kicker: "테이블 근거",
        title: table.schema ? `${table.schema}.${table.name}` : table.name,
        sentence: "테이블 이름은 읽혔지만 컬럼 구조가 없어 제약과 영향 범위를 아직 판단할 수 없습니다.",
        tone: "candidate",
        metrics: [
          { label: "테이블", value: "읽힘", tone: "green" },
          { label: "컬럼", value: "0", tone: "amber" },
          { label: "PK/FK", value: "확인 불가", tone: "gray" },
        ],
        steps: ["DB 정보 입력 또는 다시 읽기", "컬럼 목록이 채워졌는지 확인", "영향/제약 답 다시 선택"],
        note: "테이블명만으로는 컬럼 변경 영향이나 FK 제약을 판단할 수 없습니다.",
      };
    }
    return tableStructureAnswer(table, null);
  }

  if (map) {
    if (dbNeedsColumns) {
      return {
        kicker: "DB 준비 상태",
        title: "컬럼 구조 필요",
        sentence: `테이블 ${dbTableCount}개는 읽혔지만 컬럼이 없어 제약/영향 근거를 열 수 없습니다.`,
        tone: "candidate",
        metrics: [
          { label: "테이블", value: String(dbTableCount), tone: "green" },
          { label: "컬럼", value: "0", tone: "amber" },
          { label: "해야 할 일", value: "DB 입력", tone: "gray" },
        ],
        steps: ["DB 정보 입력", "다시 읽기", "컬럼 카드 확인"],
        note: "검색보다 컬럼 구조 보강이 먼저입니다.",
      };
    }
    if (dbMissingColumnTables > 0) {
      return {
        kicker: "DB 준비 상태",
        title: "컬럼 보강 필요",
        sentence: `테이블 ${dbTableCount}개 중 ${dbMissingColumnTables}개는 컬럼이 없어 해당 테이블의 제약/영향 근거를 아직 판단할 수 없습니다.`,
        tone: "candidate",
        metrics: [
          { label: "테이블", value: String(dbTableCount), tone: "green" },
          { label: "보강 필요", value: String(dbMissingColumnTables), tone: "amber" },
          { label: "해야 할 일", value: "다시 읽기", tone: "gray" },
        ],
        steps: ["컬럼 없는 테이블 확인", "DB 정보 입력 또는 다시 읽기", "대상 테이블 다시 선택"],
        note: "컬럼이 있는 테이블은 볼 수 있지만, 누락 테이블은 영향 판단에서 빠질 수 있습니다.",
      };
    }
    if (map.nodes.length === 0 && (codeItemCount > 0 || dbTableCount > 0)) {
      const firstStep = codeItemCount > 0 && dbTableCount > 0
        ? "코드 또는 테이블 카드 선택"
        : codeItemCount > 0
          ? "코드 카드 선택"
          : "테이블 카드 선택";
      return {
        kicker: "코드/DB 목록",
        title: "관계 없음",
        sentence: codeItemCount > 0 && dbTableCount > 0
          ? "관계는 아직 없지만 코드와 테이블 카드는 있습니다. 하나를 선택하면 파일/컬럼 근거부터 좁힙니다."
          : codeItemCount > 0
            ? "관계는 아직 없지만 코드 카드는 있습니다. 하나를 선택하면 파일 위치부터 확인합니다."
            : "관계는 아직 없지만 테이블 카드는 있습니다. 하나를 선택하면 컬럼/키 구조부터 확인합니다.",
        tone: "neutral",
        metrics: [
          { label: "코드", value: String(codeItemCount), tone: codeItemCount ? "green" : "gray" },
          { label: "테이블", value: String(dbTableCount), tone: dbTableCount ? "green" : "gray" },
          { label: "관계", value: "0", tone: "gray" },
        ],
        steps: [firstStep, codeItemCount > 0 ? "파일/라인 확인" : "컬럼/키 확인", "관계가 생기면 직접/후보 분리"],
      };
    }
    const edgeCounts = mapEdgeCounts(map);
    if (map.edges.length === 0) {
      const firstStep = codeItemCount > 0 && dbTableCount > 0
        ? "코드 또는 테이블 카드 선택"
        : codeItemCount > 0
          ? "코드 카드 선택"
          : dbTableCount > 0
            ? "테이블 카드 선택"
            : "캔버스 항목 선택";
      return {
        kicker: "캔버스 근거",
        title: modeLabel(map.mode),
        sentence: `관계는 아직 없고 실제 항목 ${map.nodes.length}개가 있습니다. 항목을 선택하면 파일/컬럼 근거부터 확인합니다.`,
        tone: "neutral",
        metrics: [
          { label: "항목", value: String(map.nodes.length) },
          { label: "관계", value: "0", tone: "gray" },
          { label: "구조", value: "0", tone: "gray" },
          { label: "후보", value: "0", tone: "gray" },
        ],
        steps: [firstStep, codeItemCount > 0 ? "파일/라인 확인" : "컬럼/키 확인", "관계가 생기면 직접/후보 분리"],
      };
    }
    return {
      kicker: "캔버스 근거",
      title: modeLabel(map.mode),
      sentence: "항목을 선택하면 관계와 근거가 여기로 좁혀집니다.",
      tone: "neutral",
      metrics: [
        { label: "항목", value: String(map.nodes.length) },
        { label: "관계", value: String(map.edges.length) },
        { label: "직접", value: String(edgeCounts.confirmed), tone: edgeCounts.confirmed ? "green" : "gray" },
        { label: "후보", value: String(edgeCounts.candidate), tone: edgeCounts.candidate ? "amber" : "gray" },
      ],
      steps: ["카드 또는 검색 결과 선택"],
      note: map.warnings.length > 0 ? `${map.warnings.length}개의 캔버스 경고가 있습니다.` : undefined,
    };
  }

  if (!hasWorkspace) {
    return {
      kicker: "시작",
      title: needsGithub ? "GitHub URL 필요" : "프로젝트 폴더 필요",
      sentence: needsGithub
        ? "저장소 URL을 입력하면 코드 목록부터 실제 근거로 읽습니다."
        : "프로젝트 폴더를 지정하면 코드 목록부터 실제 근거로 읽습니다.",
      tone: "neutral",
      metrics: [],
      steps: [needsGithub ? "URL 입력" : "폴더 선택", "코드 목록 만들기", "대상 선택"],
    };
  }

  return {
    kicker: "다음 행동",
    title: "대상을 선택하세요",
    sentence: "카드나 검색 결과를 선택하면 파일, 라인, 관계 근거가 여기로 좁혀집니다.",
    tone: "neutral",
    metrics: [],
    steps: ["코드 또는 DB 읽기", "대상 선택", "근거 확인"],
  };
}

function modeLabel(mode: VisualMap["mode"]): string {
  if (mode === "api-flow") return "API가 닿는 코드";
  if (mode === "table-usage") return "테이블 연결";
  if (mode === "column-impact") return "컬럼 변경 범위";
  if (mode === "search-focus") return "대상 주변 근거";
  return "전체 구조";
}

function columnStructureAnswer(table: DbInventoryTable, column: DbInventoryColumn): InspectorAnswer {
  return {
    kicker: "컬럼 구조",
    title: `${dbInventoryTableKey(table)}.${column.name}`,
    sentence: "변경 전 확인할 타입과 키 속성입니다.",
    tone: "confirmed",
    metrics: [
      { label: "타입", value: column.dataType ?? "-" },
      { label: "PK", value: column.isPrimaryKey ? "예" : "아니오", tone: column.isPrimaryKey ? "green" : "gray" },
      { label: "FK", value: column.isForeignKey ? "예" : "아니오", tone: column.isForeignKey ? "amber" : "gray" },
      { label: "NULL", value: column.nullable === null || column.nullable === undefined ? "-" : column.nullable ? "허용" : "불가" },
    ],
    steps: ["타입/NULL 변경 여부 확인"],
  };
}

function tableStructureAnswer(table: DbInventoryTable, counts: EdgeCounts | null): InspectorAnswer {
  const pkColumns = table.columns.filter((column) => column.isPrimaryKey).map((column) => column.name);
  const fkColumns = table.columns.filter((column) => column.isForeignKey).map((column) => column.name);
  return {
    kicker: "테이블",
    title: table.schema ? `${table.schema}.${table.name}` : table.name,
    sentence: counts
      ? "테이블의 키 구조와 현재 연결된 관계를 함께 봅니다. FK/PK 컬럼으로 좁히면 영향 범위가 바로 작아집니다."
      : "테이블 구조에서 키 컬럼과 FK 컬럼을 먼저 보고, 컬럼 단위 관계는 선택해서 좁혀 확인합니다.",
    tone: "confirmed",
    metrics: [
      { label: "컬럼", value: String(table.columns.length) },
      { label: "PK", value: compactColumnNames(pkColumns), tone: pkColumns.length ? "green" : "gray" },
      { label: "FK", value: compactColumnNames(fkColumns), tone: fkColumns.length ? "green" : "gray" },
      ...(counts ? [{ label: "관계", value: String(counts.confirmed + counts.typed + counts.inferred + counts.candidate), tone: counts.confirmed > 0 ? "green" as const : "gray" as const }] : []),
    ],
    steps: ["키 컬럼명 확인", "FK 컬럼으로 영향 좁히기", "관계 행에서 근거 확인"],
    note: tableKeyColumnNote(pkColumns, fkColumns),
  };
}

function domainGroupAnswer(node: VisualNode, counts: EdgeCounts): InspectorAnswer {
  const composition = domainComposition(node);
  const values = composition.match(/API (\d+) · 코드 (\d+) · DB (\d+)/);
  return {
    kicker: "도메인 구성",
    title: node.title,
    sentence: `${composition} 항목을 API → 코드 → DB 순서로 펼쳐 봅니다.`,
    tone: "neutral",
    metrics: values
      ? [
          { label: "API", value: values[1] },
          { label: "코드", value: values[2] },
          { label: "DB", value: values[3] },
          { label: "구조 관계", value: String(counts.typed), tone: "gray" },
        ]
      : nodeRelationMetrics(counts),
    steps: ["API → 코드 → DB 순서로 확인"],
    note: "도메인 포함 관계는 읽기 위한 구조이며 실제 호출이나 DB 제약을 뜻하지 않습니다.",
  };
}

function domainComposition(node: VisualNode): string {
  return node.kind === "group-domain"
    ? node.subtitle?.split("|")[0] ?? "도메인 구성"
    : node.subtitle ?? "";
}

function compactPath(value?: string | null): string | null {
  const parts = value?.split(/[\\/]+/).filter(Boolean) ?? [];
  return parts.length ? parts.slice(-3).join("/") : null;
}

function connectionCounts(map: VisualMap | null, node: VisualNode): EdgeCounts {
  const edges = map?.edges.filter((edge) => edgeTouchesNode(edge, node)) ?? [];
  return edgeCounts(edges);
}

function firstNodeRelationEdge(node: VisualNode, map: VisualMap | null): VisualEdge | null {
  const edges = map?.edges.filter((item) => edgeTouchesNode(item, node)) ?? [];
  return [...edges].sort((a, b) => relationPriority(a) - relationPriority(b))[0] ?? null;
}

function edgeCopySummary(edge: VisualEdge, map: VisualMap | null): string {
  const evidence = edge.evidence[0]?.text ?? edgeKindLabel(edge);
  return `${relationshipSourceLabel(edge)} ${edgeKindLabel(edge)}: ${endpointLabel(edge.from, map)} → ${endpointLabel(edge.to, map)} · ${evidence}`;
}

function relationPriority(edge: VisualEdge): number {
  if (!edge.kind.startsWith("candidate") && edge.kind !== "code_flow" && !isStructuralEdge(edge) && edge.evidence.length > 0) {
    return 0;
  }
  if (!edge.kind.startsWith("candidate") && edge.kind !== "code_flow") {
    return 1;
  }
  return edge.kind.startsWith("candidate") ? 2 : 3;
}

function mapEdgeCounts(map: VisualMap): EdgeCounts {
  return edgeCounts(map.edges);
}

function edgeCounts(edges: VisualEdge[]): EdgeCounts {
  const counts: EdgeCounts = { confirmed: 0, typed: 0, inferred: 0, candidate: 0 };
  for (const edge of edges) {
    if (edge.kind.startsWith("candidate")) {
      counts.candidate += 1;
    } else if (edge.kind === "code_flow") {
      counts.inferred += 1;
    } else if (isStructuralEdge(edge) || edge.evidence.length === 0) {
      counts.typed += 1;
    } else {
      counts.confirmed += 1;
    }
  }
  return counts;
}

function nodeRelationMetrics(counts: EdgeCounts): InspectorAnswer["metrics"] {
  const total = counts.confirmed + counts.typed + counts.inferred + counts.candidate;
  if (total === 0) {
    return [{ label: "관계", value: "0", tone: "gray" }];
  }
  return [
    ...(counts.confirmed > 0 ? [{ label: "직접", value: String(counts.confirmed), tone: "green" as const }] : []),
    ...(counts.typed > 0 ? [{ label: "구조", value: String(counts.typed), tone: "gray" as const }] : []),
    ...(counts.candidate > 0 ? [{ label: "후보", value: String(counts.candidate), tone: "amber" as const }] : []),
    ...(counts.inferred > 0 ? [{ label: "이름 단서", value: String(counts.inferred), tone: "gray" as const }] : []),
  ];
}

function nodeKindLabel(kind: string): string {
  if (kind === "group-domain") {
    return "도메인";
  }
  if (kind === "api") {
    return "API";
  }
  if (kind === "table") {
    return "테이블";
  }
  if (kind === "column") {
    return "컬럼";
  }
  if (kind === "file") {
    return "파일";
  }
  return "코드";
}

function nodeAnswerSentence(node: VisualNode, counts: EdgeCounts, map: VisualMap | null): string {
  if (node.kind === "group-domain") {
    return "이 도메인에 묶인 API, 코드, DB 항목을 읽는 순서대로 보여줍니다.";
  }
  if (node.kind === "api") {
    return "이 API가 닿는 코드와 DB 근거입니다.";
  }
  if (node.kind === "table") {
    if (counts.candidate > 0) {
      return nodeHasCodeRelation(node, map)
        ? "이 테이블의 직접 FK와 코드 후보를 분리해 봅니다."
        : "이 테이블의 직접 FK와 후보 근거를 분리해 봅니다.";
    }
    return nodeHasCodeRelation(node, map)
      ? "이 테이블의 코드 후보와 직접 DB 근거입니다."
      : "이 테이블의 직접 DB 근거입니다.";
  }
  if (node.kind === "column") {
    return nodeHasCodeRelation(node, map)
      ? "이 컬럼 변경이 닿는 직접/후보 근거입니다."
      : "이 컬럼의 직접/후보 근거입니다.";
  }
  if (node.kind === "file") {
    return "이 파일 주변의 심볼과 호출 관계입니다.";
  }
  return "이 코드 항목 주변의 호출과 DB 후보 근거입니다.";
}

function nodeAnswerSteps(node: VisualNode, map: VisualMap | null): string[] {
  if (node.kind === "group-domain") {
    return ["API → 코드 → DB 순서로 확인"];
  }
  if (node.kind === "column") {
    return nodeHasCodeRelation(node, map)
      ? ["직접 제약 먼저 확인"]
      : ["직접 제약 먼저 확인"];
  }
  if (node.kind === "table") {
    if (nodeHasCodeRelation(node, map)) {
      return ["이 테이블에 닿는 코드 확인"];
    }
    return ["FK/제약 관계 확인"];
  }
  if (node.kind === "api") {
    return ["연결된 코드 확인"];
  }
  return ["직접 호출 관계 확인"];
}

function codeAnswerSteps(code: CodeInventoryItem): string[] {
  const kind = code.kind.trim().toLowerCase();
  const relationStep = kind === "api" || kind === "route" ? "API가 닿는 코드 또는 캔버스에서 확인" : "캔버스에서 주변 관계 확인";
  return ["파일/라인을 에디터에서 열기", relationStep, "DB 후보는 근거 라벨로 직접/후보 구분"];
}

function compactColumnNames(names: string[]): string {
  if (names.length === 0) {
    return "-";
  }
  if (names.length <= 2) {
    return names.join(", ");
  }
  return `${names[0]} 외 ${names.length - 1}`;
}

function tableKeyColumnNote(pkColumns: string[], fkColumns: string[]): string {
  const parts = [
    pkColumns.length ? `PK: ${pkColumns.join(", ")}` : null,
    fkColumns.length ? `FK: ${fkColumns.join(", ")}` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "PK/FK 컬럼이 잡히지 않았습니다.";
}

function relationshipSourceLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return "후보";
  }
  if (edge.kind === "code_flow") {
    return "이름 단서";
  }
  if (isStructuralEdge(edge)) {
    return "구조";
  }
  return edge.evidence.length > 0 ? "직접" : "구조";
}

function isStructuralEdge(edge: VisualEdge): boolean {
  return edge.kind === "contains" || edge.kind === "group_contains" || edge.kind.startsWith("structural_");
}

function edgeTrustLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return `후보 ${confidenceLabel(edge.confidence) ?? "낮음"}`;
  }
  if (edge.kind === "code_flow") {
    return "이름 단서";
  }
  if (isStructuralEdge(edge)) {
    return "구조 근거";
  }
  return edge.evidence.length > 0 ? "근거 있음" : "구조 근거";
}

function edgeKindLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return "후보 근거";
  }
  if (edge.kind === "contains" || edge.kind === "group_contains") {
    return "포함 관계";
  }
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") {
    return "DB 제약";
  }
  if (edge.kind === "code_call") {
    return "코드 호출";
  }
  if (edge.kind === "code_handle") {
    return "라우트 처리";
  }
  if (edge.kind === "code_flow") {
    return "이름 단서";
  }
  return "관계";
}

function edgeTrustTone(edge: VisualEdge): "green" | "amber" | "gray" {
  if (edge.kind.startsWith("candidate")) {
    return confidenceBadgeTone(edge.confidence);
  }
  return edge.kind === "code_flow" || isStructuralEdge(edge) || edge.evidence.length === 0 ? "gray" : "green";
}

function edgeTrustReason(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return confidenceReasonLabel(edge.confidence);
  }
  if (edge.kind === "code_flow") {
    return "이름이 비슷해 이어 둔 후보 근거입니다.";
  }
  if (isStructuralEdge(edge)) {
    return edge.evidence[0]?.text ?? "프로젝트를 읽기 쉽게 묶은 구조 근거입니다.";
  }
  return edge.evidence[0]?.text ?? "상세 근거 문장 없이 구조 근거만 표시합니다.";
}

function edgeEvidenceTone(edge: VisualEdge): "candidate" | "confirmed" | "neutral" {
  if (edge.kind.startsWith("candidate")) {
    return "candidate";
  }
  return edge.kind === "code_flow" || isStructuralEdge(edge) || edge.evidence.length === 0 ? "neutral" : "confirmed";
}

function emptyEvidenceLabel(edge: VisualEdge): string {
  return edge.kind.startsWith("candidate")
    ? "근거 문장 없음 · 후보 근거만 표시"
    : "근거 문장 없음 · 구조 근거만 표시";
}

function endpointLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  if (!node) {
    return columnLabelFromNodeId(id) ?? (id.startsWith("db:table:") ? id.slice("db:table:".length) : id);
  }
  const title = nodeDisplayTitle(node);
  return node.kind === "column" ? title : node.subtitle ? `${title} (${node.subtitle})` : title;
}

function endpointTitleLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  return node ? nodeDisplayTitle(node) : columnLabelFromNodeId(id) ?? (id.startsWith("db:table:") ? id.slice("db:table:".length) : id);
}

function nodeDisplayTitle(node: VisualNode): string {
  if (node.kind !== "column") {
    return node.title;
  }
  const tableKey = tableKeyFromColumnNodeId(node.id);
  return tableKey ? `${tableKey}.${node.title}` : node.title;
}

function tableKeyFromColumnNodeId(nodeId: string): string | null {
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? body.slice(0, splitIndex) : null;
}

function columnRefFromNodeId(nodeId: string): { tableKey: string; columnName: string } | null {
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 && splitIndex < body.length - 1
    ? { tableKey: body.slice(0, splitIndex), columnName: body.slice(splitIndex + 1) }
    : null;
}

function tableKeyFromDbNodeId(nodeId: string): string | null {
  if (nodeId.startsWith("db:table:")) {
    return nodeId.slice("db:table:".length);
  }
  return tableKeyFromColumnNodeId(nodeId);
}

function firstTableColumnAction(
  table: DbInventoryTable,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): InspectorAction | null {
  const column =
    table.columns.find((item) => item.isForeignKey) ??
    table.columns.find((item) => item.isPrimaryKey) ??
    table.columns[0] ??
    null;
  if (!column) {
    return null;
  }
  const tableKey = dbInventoryTableKey(table);
  const label = column.isForeignKey
    ? `${column.name} FK 범위`
    : column.isPrimaryKey
      ? `${column.name} PK 제약`
      : `${column.name} 변경 범위`;
  return {
    label,
    run: () => {
      dbProfileControls.selectColumn(tableKey, column.name);
      visualMapControls.showMode("column-impact", `db:column:${tableKey}:${column.name}`);
    },
    primary: true,
    disabled: dbProfileControls.busy,
  };
}

function columnLabelFromNodeId(nodeId: string): string | null {
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? `${body.slice(0, splitIndex)}.${body.slice(splitIndex + 1)}` : null;
}

function nodeSourceLabel(source: string): string {
  if (source === "code") {
    return "코드";
  }
  if (source === "db") {
    return "DB 구조";
  }
  if (source === "projection") {
    return "자동 묶음";
  }
  return source;
}

function relationshipReason(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return edge.evidence[0]?.text ?? "이름이 비슷해 이어 둔 후보 근거입니다";
  }
  if (isStructuralEdge(edge)) {
    return edge.evidence[0]?.text ?? "프로젝트를 읽기 위한 포함/그룹 구조입니다";
  }
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") {
    return edge.evidence[0]?.text ?? "DB 제약 구조입니다";
  }
  if (edge.kind === "code_call") {
    return edge.evidence[0]?.text ?? "읽은 호출 구조입니다. 상세 근거 문장은 없습니다";
  }
  if (edge.kind === "code_handle") {
    return edge.evidence[0]?.text ?? "코드 엔진에서 읽은 HANDLES 관계입니다";
  }
  if (edge.kind === "code_flow") {
    return edge.evidence[0]?.text ?? "이름 단서로 이어 둔 연결입니다";
  }
  return edge.evidence[0]?.text ?? "이름과 구조 단서로 연결했습니다";
}

function copyValuesForNode(node: VisualNode): Array<[string, string]> {
  if (node.kind === "group-domain") {
    return [["도메인", node.title], ["구성", domainComposition(node)], ["ID", node.id]];
  }
  if (node.kind === "api") {
    return [["라우트", node.title], ["ID", node.id], ["경로", node.subtitle ?? ""]];
  }
  if (node.kind === "table") {
    return [["테이블", node.title], ["스키마", node.subtitle ?? ""], ["ID", node.id]];
  }
  if (node.kind === "column") {
    return [["컬럼", nodeDisplayTitle(node)], ["타입", node.subtitle ?? ""], ["ID", node.id]];
  }
  if (node.kind === "file") {
    return [["경로", node.subtitle ?? node.title], ["ID", node.id]];
  }
  return [["심볼", node.title], ["경로", node.subtitle ?? ""], ["ID", node.id]];
}

function nodeEvidenceSummary(
  node: VisualNode,
  map: VisualMap | null,
): {
  confidence: string;
  badgeTone: "green" | "amber" | "gray";
  connectionSummary: string;
  evidence: Array<{ key: string; text: string; tone: "confirmed" | "candidate" | "neutral" }>;
  relatedFiles: string[];
} {
  const relatedEdges = map?.edges.filter((edge) => edgeTouchesNode(edge, node)) ?? [];
  const candidateEdges = relatedEdges.filter((edge) => edge.kind.startsWith("candidate"));
  const inferredEdges = relatedEdges.filter((edge) => edge.kind === "code_flow");
  const confirmedEdges = relatedEdges.filter(
    (edge) => !edge.kind.startsWith("candidate") && edge.kind !== "code_flow" && !isStructuralEdge(edge) && edge.evidence.length > 0,
  );
  const typedEdges = relatedEdges.filter(
    (edge) => !edge.kind.startsWith("candidate") && edge.kind !== "code_flow" && (isStructuralEdge(edge) || edge.evidence.length === 0),
  );
  const candidateConfidence = strongestCandidateConfidence(candidateEdges);
  const nodeTrust = nodeTrustSummary({
    candidateConfidence,
    confirmedCount: confirmedEdges.length,
    typedCount: typedEdges.length,
    inferredCount: inferredEdges.length,
    source: node.source,
  });
  const relatedFiles = node.source === "code" && node.subtitle ? [node.subtitle] : [];

  return {
    confidence: nodeTrust.label,
    badgeTone: nodeTrust.tone,
    connectionSummary: `직접 ${confirmedEdges.length} · 구조 ${typedEdges.length} · 후보 ${candidateEdges.length} · 이름 단서 ${inferredEdges.length}`,
    evidence: [
      {
        key: "source",
        text: `${nodeSourceLabel(node.source)} 읽은 항목: ${nodeDisplayTitle(node)}${node.subtitle ? ` (${domainComposition(node)})` : ""}`,
        tone: node.source === "projection" ? "neutral" : "confirmed",
      },
      ...relatedEdges.slice(0, 3).map((edge) => ({
        key: edge.id,
        text: `${relationshipSourceLabel(edge)}: ${endpointLabel(edge.from, map)} → ${endpointLabel(edge.to, map)} · ${edge.evidence[0]?.text ?? edgeKindLabel(edge)}`,
        tone: edgeEvidenceTone(edge),
      })),
    ],
    relatedFiles,
  };
}

function nodeTrustSummary({
  candidateConfidence,
  confirmedCount,
  typedCount,
  inferredCount,
  source,
}: {
  candidateConfidence: "high" | "medium" | "low" | null;
  confirmedCount: number;
  typedCount: number;
  inferredCount: number;
  source: VisualNode["source"];
}): { label: string; tone: "green" | "amber" | "gray" } {
  if (candidateConfidence) {
    return { label: `후보 ${confidenceLabel(candidateConfidence) ?? "낮음"}`, tone: confidenceBadgeTone(candidateConfidence) };
  }
  if (confirmedCount > 0) {
    return { label: "근거 있음", tone: "green" };
  }
  if (typedCount > 0) {
    return { label: "구조 근거", tone: "gray" };
  }
  if (inferredCount > 0 || source === "projection") {
    return { label: "이름 단서", tone: "gray" };
  }
  return { label: "읽은 항목", tone: "gray" };
}

function edgeTouchesNode(edge: VisualEdge, node: VisualNode): boolean {
  if (edge.from === node.id || edge.to === node.id) {
    return true;
  }
  if (node.kind !== "table" || !node.id.startsWith("db:table:")) {
    return false;
  }
  const tableKey = node.id.slice("db:table:".length);
  const columnPrefix = `db:column:${tableKey}:`;
  return edge.from.startsWith(columnPrefix) || edge.to.startsWith(columnPrefix);
}

function nodeHasCodeRelation(node: VisualNode, map: VisualMap | null): boolean {
  return Boolean(
    map?.edges.some((edge) => edgeTouchesNode(edge, node) && (edge.from.startsWith("code:") || edge.to.startsWith("code:"))),
  );
}

function edgeHasCodeEndpoint(edge: VisualEdge): boolean {
  return edge.from.startsWith("code:") || edge.to.startsWith("code:");
}

function strongestCandidateConfidence(edges: VisualEdge[]): "high" | "medium" | "low" | null {
  if (edges.some((edge) => normalizeConfidence(edge.confidence) === "high")) {
    return "high";
  }
  if (edges.some((edge) => normalizeConfidence(edge.confidence) === "medium")) {
    return "medium";
  }
  return edges.length > 0 ? "low" : null;
}

function columnImpactSummary(node: VisualNode, map: VisualMap | null): {
  directCount: number;
  candidateCount: number;
  constraints: string;
} {
  const connectedEdges = map?.edges.filter((edge) => edge.from === node.id || edge.to === node.id) ?? [];
  const directCount = connectedEdges.filter((edge) => !edge.kind.startsWith("candidate")).length;
  const candidateCount = connectedEdges.filter((edge) => edge.kind.startsWith("candidate")).length;
  const fkCount = connectedEdges.filter((edge) => edge.kind === "db_fk" || edge.kind === "db_constraint").length;
  const constraints = fkCount > 0 ? `FK 관계 ${fkCount}개` : "-";
  return { directCount, candidateCount, constraints };
}
