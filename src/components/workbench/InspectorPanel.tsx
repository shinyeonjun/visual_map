import { Copy, FileText, Info, LoaderCircle, MousePointer2, Type } from "lucide-react";
import { useState } from "react";
import { dbProfileWorkStarted } from "../../types/controls";
import { codeInventoryItemCount, dbInventoryTableKey, dbProfileSourceLabel } from "../../types/workspace";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { ApiReadingAnswer, ImpactReviewItem, VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  visualEdgeKindLabel as edgeKindLabel,
  visualNodeKindLabel as nodeKindLabel,
} from "../../visual/labels";
import { columnRefFromNodeId, tableKeyFromDbNodeId } from "../../visual/nodeIds";
import { copyValue } from "../common/copyValue";
import { focusDbProfileSetup as focusDbProfileInput } from "../common/focusSourceSetup";
import {
  columnImpactSummary,
  compactPath,
  copyValuesForNode,
  edgeCopySummary,
  edgeEvidenceTone,
  edgeTrustLabel,
  edgeTrustReason,
  edgeTrustTone,
  emptyEvidenceLabel,
  endpointLabel,
  firstNodeRelationEdge,
  firstTableColumnAction,
  inspectorAnswer,
  nodeDisplayTitle,
  nodeEvidenceSummary,
  nodeHasCodeRelation,
  nodeSourceLabel,
  relationshipReason,
  relationshipSourceLabel,
  type InspectorAction,
  type InspectorAnswer,
} from "./inspectorModel";
import { SourceJumpRow } from "./SourceJumpRow";

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
  if (visualMapControls.loading) {
    return (
      <section className="side-card inspector" aria-busy="true">
        <div className="panel-header">
          <Info size={16} />
          <h2>선택 근거</h2>
        </div>
        <div className="evidence-transition-state" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={20} />
          <strong>근거를 불러오는 중</strong>
          <span>새 화면과 일치하는 정보만 표시합니다.</span>
        </div>
      </section>
    );
  }

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
      ? firstTableColumnAction(selectedTable, dbProfileControls)
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
        <h2>선택 근거</h2>
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
                    <span>직접 관계</span>
                    <strong>{columnImpact.directCount}개</strong>
                    <span>후보 관계</span>
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
                    <span>직접 관계</span>
                    <strong>{columnImpact.directCount}개</strong>
                    <span>후보 관계</span>
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

function focusGlobalSearch(visualMapControls: VisualMapControls) {
  visualMapControls.openSearchPopover();
  window.requestAnimationFrame(() => {
    const target = document.getElementById("global-inventory-search") as HTMLInputElement | null;
    target?.focus();
    target?.select();
  });
}

function firstVisibleNodeActionLabel(node: VisualNode): string {
  return `첫 ${nodeKindLabel(node.kind)} 보기`;
}

function hasSearchableInventory(workspaceControls: WorkspaceControls, dbProfileControls: DbProfileControls): boolean {
  return codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
}
