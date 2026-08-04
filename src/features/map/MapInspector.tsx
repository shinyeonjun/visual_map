import { ArrowRight, FileText, GitBranch, Info, MousePointer2, Pin, TriangleAlert, Type, X } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import {
  codeRouteMethod,
  codeInventoryItemCount,
  dbInventoryTableKey,
  dbProfileSourceLabel,
  routeDisplayName,
  routeMethodFromIdentity,
} from "../../types/workspace";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import type { VisualEdge, VisualNode } from "../../types/visual-map";
import type { DbInventoryTable } from "../../types/workspace";
import { visualEdgeKindLabel as edgeKindLabel, visualNodeKindLabel as nodeKindLabel } from "../../visual/labels";
import { columnRefFromNodeId, tableKeyFromDbNodeId } from "../../visual/nodeIds";
import { focusDbProfileSetup as focusDbProfileInput } from "../../components/common/focusSourceSetup";
import {
  columnImpactSummary,
  compactPath,
  copyValuesForNode,
  edgeCopySummary,
  edgeEvidenceTone,
  edgeTrustLabel,
  edgeTrustReason,
  edgeTrustTone,
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
} from "./mapInspectorModel";
import { SourceJump } from "./SourceJump";
import {
  AnswerSummary,
  apiEdgeLabel,
  codeInventoryItemById,
  codeInventoryItemFromNode,
  CopyRow,
  edgeEndpointAction,
  InspectorEmptyRow,
  InspectorSection,
  InspectorUpdating,
  inspectorEmptyAction,
  inspectorEvidenceText,
  requiresReview,
  uniqueInspectorEvidence,
} from "./MapInspectorParts";

/**
 * How many rows each inspector list shows before the rest move behind a
 * disclosure. The lists no longer scroll inside themselves, so this count is
 * what keeps a 35-relation selection from burying the sections under it.
 */
const INSPECTOR_LIST_PREVIEW = 6;

export function MapInspector({
  onClose,
  title = "선택한 대상",
  variant = "full",
  showDbSetup,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  focusedGroup,
}: {
  onClose?: () => void;
  title?: string;
  variant?: "full" | "answer";
  showDbSetup?: () => void;
  showWorkspaceSetup?: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  focusedGroup?: VisualNode | null;
}) {
  const selectedEdge = visualMapControls.selectedEdge;
  const visibleMode = visualMapControls.currentMap?.mode ?? visualMapControls.mode;
  const apiReading = visibleMode === "api-flow" ? (visualMapControls.currentMap?.apiReading ?? null) : null;
  const analysisFocusId =
    visibleMode === "composition"
      ? (visualMapControls.selectedNode?.id ?? "")
      : visualMapControls.loading && visualMapControls.currentMap
        ? visualMapControls.currentMap.focus
        : (visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? "");
  const selectedNode =
    visualMapControls.selectedNode ??
    (!selectedEdge && apiReading
      ? (visualMapControls.currentMap?.nodes.find((node) => node.id === visualMapControls.currentMap?.focus) ?? null)
      : !selectedEdge && focusedGroup
        ? focusedGroup
        : null);
  const selectedEdgeFrom = selectedEdge ? endpointLabel(selectedEdge.from, visualMapControls.currentMap) : null;
  const selectedEdgeTo = selectedEdge ? endpointLabel(selectedEdge.to, visualMapControls.currentMap) : null;
  const columnImpact =
    selectedNode?.kind === "column" ? columnImpactSummary(selectedNode, visualMapControls.currentMap) : null;
  const selectedNodeHasCodeRelation = selectedNode
    ? nodeHasCodeRelation(selectedNode, visualMapControls.currentMap)
    : false;
  const nodeEvidence = selectedNode ? nodeEvidenceSummary(selectedNode, visualMapControls.currentMap) : null;
  const focusedNodeId = selectedNode?.id ?? analysisFocusId;
  const focusedCodeId =
    selectedNode?.source === "code"
      ? selectedNode.id.replace(/^code:/, "")
      : !selectedNode && focusedNodeId.startsWith("code:")
        ? focusedNodeId.replace(/^code:/, "")
        : null;
  const focusedMapNode = visualMapControls.currentMap?.nodes.find((node) => node.id === focusedNodeId) ?? null;
  const selectedCode =
    codeInventoryItemById(workspaceControls.codeInventory, focusedCodeId) ??
    codeInventoryItemFromNode(selectedNode ?? focusedMapNode);
  const apiMethod = apiReading?.method ?? routeMethodFromIdentity(focusedNodeId);
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
  const focusedColumn =
    selectedNode?.kind === "column"
      ? columnRefFromNodeId(selectedNode.id)
      : !selectedNode
        ? columnRefFromNodeId(focusedNodeId)
        : null;
  const selectedNodeTableKey = selectedNode
    ? tableKeyFromDbNodeId(selectedNode.id)
    : (focusedColumn?.tableKey ?? tableKeyFromDbNodeId(focusedNodeId));
  const useSelectedTable =
    focusedNodeId.startsWith("db:") || visibleMode === "table-usage" || visibleMode === "column-impact";
  const selectedTable =
    dbTables.find((table) => dbInventoryTableKey(table) === selectedNodeTableKey) ??
    (!useSelectedTable || focusedNodeId.startsWith("code:")
      ? null
      : dbTables.find((table) => dbInventoryTableKey(table) === dbProfileControls.selectedTableKey)) ??
    null;
  const selectedColumn =
    focusedColumn && selectedTable && dbInventoryTableKey(selectedTable) === focusedColumn.tableKey
      ? (selectedTable.columns.find((column) => column.name === focusedColumn.columnName) ?? null)
      : null;
  const tableColumnAction =
    !selectedEdge &&
    !selectedCode &&
    !selectedColumn &&
    selectedTable &&
    (!selectedNode || selectedNode.kind === "table")
      ? firstTableColumnAction(selectedTable, dbProfileControls)
      : null;
  const answer = inspectorAnswer({
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
    apiMethod,
  });
  const hasSelection = Boolean(selectedEdge || selectedNode || selectedCode || selectedColumn || selectedTable);
  const emptyAction = hasSelection
    ? null
    : inspectorEmptyAction(workspaceControls, dbProfileControls, visualMapControls, dbSetupAction);
  const firstNodeRelation = selectedNode ? firstNodeRelationEdge(selectedNode, visualMapControls.currentMap) : null;
  const selectedEdgeAction = selectedEdge
    ? edgeEndpointAction(selectedEdge, visualMapControls.currentMap, visualMapControls.selectNode)
    : null;
  const selectionAction = selectedEdgeAction
    ? selectedEdgeAction
    : (selectedTable?.columns.length === 0 || (selectedNode?.kind === "table" && dbNeedsColumns)) && dbColumnAction
      ? dbColumnAction
      : tableColumnAction
        ? tableColumnAction
        : !selectedEdge && firstNodeRelation
          ? { label: "근거 보기", run: () => visualMapControls.selectEdge(firstNodeRelation), primary: true }
          : null;
  const relationTargetId = selectedNode?.id ?? (hasSelection ? focusedNodeId || null : null);
  const directEdges = relationTargetId
    ? (visualMapControls.currentMap?.edges.filter(
        (edge) => edge.from === relationTargetId || edge.to === relationTargetId,
      ) ?? [])
    : [];
  const renderDirectRelation = (edge: VisualEdge) => {
    const outbound = edge.from === relationTargetId;
    const otherLabel = endpointLabel(outbound ? edge.to : edge.from, visualMapControls.currentMap);
    return (
      <button
        className={edgeEvidenceTone(edge)}
        type="button"
        onClick={() => visualMapControls.selectEdge(edge)}
        key={edge.id}
      >
        <ArrowRight className={outbound ? "" : "inbound"} size={14} />
        <span>
          <b>{apiEdgeLabel(edge)}</b>
          <small title={otherLabel}>{otherLabel}</small>
        </span>
      </button>
    );
  };
  const directEvidence = uniqueInspectorEvidence(
    directEdges.flatMap((edge) =>
      edge.evidence.map((item) => ({
        key: `${item.kind}:${item.text}`,
        text: inspectorEvidenceText(item.kind, item.text),
        tone: edgeEvidenceTone(edge),
      })),
    ),
  );
  const reviewBoardEvidence = uniqueInspectorEvidence(
    (visualMapControls.currentMap?.reviewBoard?.lanes.find((lane) => lane.id === "direct")?.items ?? [])
      .filter((item) => !selectedNode || !item.nodeId || item.nodeId === selectedNode.id)
      .map((item) => ({
        key: item.id,
        text: `${item.title} · ${item.detail}`,
        tone: item.truthClass === "confirmed" ? "confirmed" : "neutral",
      })),
  );
  const apiStep = selectedNode ? (apiReading?.steps.find((step) => step.nodeId === selectedNode.id) ?? null) : null;
  const apiStepEvidence = apiStep
    ? apiStep.evidence.map((item) => ({
        key: `${item.kind}:${item.text}`,
        text: inspectorEvidenceText(item.kind, item.text),
        tone:
          apiStep.truthClass === "candidate"
            ? "candidate"
            : apiStep.truthClass === "confirmed"
              ? "confirmed"
              : "neutral",
      }))
    : [];
  const nodeEvidenceItems = uniqueInspectorEvidence([...(nodeEvidence?.evidence ?? []), ...apiStepEvidence]);
  const evidenceItems: Array<{ key: string; text: string; tone: string }> = selectedEdge
    ? selectedEdge.evidence.map((item) => ({
        key: `${item.kind}:${item.text}`,
        text: inspectorEvidenceText(item.kind, item.text),
        tone: edgeEvidenceTone(selectedEdge),
      }))
    : nodeEvidenceItems.length
      ? nodeEvidenceItems
      : directEvidence.length
        ? directEvidence
        : reviewBoardEvidence;
  const edgeCodeNode = selectedEdge
    ? ([selectedEdge.from, selectedEdge.to]
        .map((id) => visualMapControls.currentMap?.nodes.find((node) => node.id === id) ?? null)
        .find((node) => node?.source === "code") ?? null)
    : null;
  const sourceCode = edgeCodeNode
    ? (codeInventoryItemById(workspaceControls.codeInventory, edgeCodeNode.id.replace(/^code:/, "")) ??
      codeInventoryItemFromNode(edgeCodeNode))
    : selectedCode;
  const apiNextCheck = apiReading
    ? selectedNode
      ? (apiReading.recommendedChecks.find((item) => item.nodeId === selectedNode.id) ??
        apiReading.recommendedChecks[0] ??
        null)
      : (apiReading.recommendedChecks[0] ?? null)
    : null;
  const reviewNextCheck =
    visualMapControls.currentMap?.reviewBoard?.lanes.find((lane) => lane.id === "checks")?.items[0] ?? null;
  const suggestedCheck = apiNextCheck ?? reviewNextCheck;
  const suggestedNode = suggestedCheck?.nodeId
    ? (visualMapControls.currentMap?.nodes.find((node) => node.id === suggestedCheck.nodeId) ?? null)
    : null;
  const nextAction = selectionAction ?? emptyAction;
  const selectedEdgeNodes = selectedEdge
    ? [selectedEdge.from, selectedEdge.to]
        .map((id) => visualMapControls.currentMap?.nodes.find((node) => node.id === id) ?? null)
        .filter((node): node is VisualNode => Boolean(node))
    : [];
  const hasCandidateRelation = selectedEdge ? requiresReview(selectedEdge) : directEdges.some(requiresReview);
  const nextCheckText = answer.steps[1] ?? null;
  const selectionKey =
    selectedEdge?.id ??
    selectedNode?.id ??
    selectedCode?.id ??
    (selectedColumn && selectedTable ? `${dbInventoryTableKey(selectedTable)}.${selectedColumn.name}` : null) ??
    (selectedTable ? dbInventoryTableKey(selectedTable) : "none");
  const [pinned, setPinned] = useState(false);
  const selectedIdentity = inspectorIdentity({
    selectedEdge,
    selectedNode,
    selectedColumn,
    selectedTable,
    selectedCode,
    edgeFrom: selectedEdgeFrom,
    edgeTo: selectedEdgeTo,
    apiMethod,
  });

  return (
    <section
      className={`side-card inspector${variant === "answer" ? " answer-inspector" : ""}${visualMapControls.loading ? " is-refreshing" : ""}`}
      aria-busy={visualMapControls.loading}
    >
      <div className="panel-header">
        <Info size={16} />
        <h2>{title}</h2>
        <button
          className={`inspector-pin${pinned ? " active" : ""}`}
          type="button"
          aria-label="인스펙터 고정"
          aria-pressed={pinned}
          title={pinned ? "인스펙터 고정 해제" : "인스펙터 고정"}
          onClick={() => setPinned((value) => !value)}
        >
          <Pin size={14} />
        </button>
        {onClose ? (
          <button className="inspector-close" type="button" onClick={onClose} aria-label="선택 해제" title="선택 해제">
            <X size={15} />
          </button>
        ) : null}
      </div>
      {visualMapControls.loading ? <InspectorUpdating /> : null}
      <div className="inspector-scroll-body">
        {/*
          What is selected has to be readable without opening anything. It used
          to live inside the "선택 상세" disclosure, which meant the panel could
          show evidence for a target the reader could not name.
        */}
        {selectedIdentity ? (
          <div className="inspector-identity" data-identity-kind={selectedIdentity.kind}>
            <span className="inspector-identity-mark" aria-hidden="true">
              {selectedIdentity.icon}
            </span>
            <div className="inspector-identity-copy">
              <div className="inspector-identity-title">
                <strong title={selectedIdentity.title}>{selectedIdentity.title}</strong>
                <em>{selectedIdentity.kindLabel}</em>
              </div>
              {selectedIdentity.id ? <code title={selectedIdentity.id}>{selectedIdentity.id}</code> : null}
              {selectedIdentity.detail ? <small>{selectedIdentity.detail}</small> : null}
            </div>
          </div>
        ) : null}
        <InspectorSection title={variant === "answer" ? "선택" : "요약"}>
          <AnswerSummary answer={answer} hideTitle={Boolean(selectedIdentity)} />
          {hasSelection && (
            <details className="inspector-details" key={selectionKey}>
              <summary>선택 상세</summary>
              <div className="inspector-details-body">
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
                    <CopyRow
                      values={[
                        ["관계", edgeCopySummary(selectedEdge, visualMapControls.currentMap)],
                        ["기준", selectedEdge.from],
                        ["연결 대상", selectedEdge.to],
                      ]}
                    />
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
                      <strong>
                        {selectedColumn.nullable === null || selectedColumn.nullable === undefined
                          ? "-"
                          : selectedColumn.nullable
                            ? "허용"
                            : "불가"}
                      </strong>
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
                        <span className="secret-note">후보 근거는 이름 기반이며 직접 증거가 아닙니다.</span>
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
                      <strong className={`badge ${nodeEvidence?.badgeTone ?? "gray"}`}>
                        {nodeEvidence?.confidence ?? "-"}
                      </strong>
                      <span>연결</span>
                      <strong>{nodeEvidence?.connectionSummary ?? "-"}</strong>
                    </div>
                    {nodeEvidence?.relatedFiles.length ? (
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
                    ) : null}
                    <CopyRow values={copyValuesForNode(selectedNode)} />
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
                        <span className="secret-note">후보 근거는 이름 기반이며 직접 증거가 아닙니다.</span>
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
                      <strong title={selectedCode.filePath ?? undefined}>
                        {compactPath(selectedCode.filePath) ?? "-"}
                      </strong>
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
                  </>
                ) : selectedTable ? (
                  <>
                    <div className="edge-summary">
                      <code>
                        {selectedTable.schema ? `${selectedTable.schema}.${selectedTable.name}` : selectedTable.name}
                      </code>
                    </div>
                    <div className="kv">
                      <span>컬럼</span>
                      <strong>{selectedTable.columns.length}</strong>
                      <span>연결</span>
                      <strong className="badge green">{dbProfileControls.activeProfile?.name ?? "활성"}</strong>
                      <span>출처</span>
                      <strong>
                        {dbProfileControls.activeProfile
                          ? dbProfileSourceLabel(dbProfileControls.activeProfile.source)
                          : "-"}
                      </strong>
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
                        <span key={dbInventoryTableKey(selectedTable) + ":" + column.name}>
                          <Type size={13} />
                          {column.name}
                          <em>{column.dataType ?? (column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "")}</em>
                        </span>
                      ))}
                    </div>
                  </>
                ) : null}
              </div>
            </details>
          )}
        </InspectorSection>

        {hasSelection ? (
          <>
            {variant === "full" ? (
              <InspectorSection title="바로 연결" count={selectedEdge ? selectedEdgeNodes.length : directEdges.length}>
                {selectedEdge ? (
                  selectedEdgeNodes.length > 0 ? (
                    <div className="inspector-edge-endpoints">
                      {selectedEdgeNodes.map((node) => (
                        <button type="button" onClick={() => visualMapControls.selectNode(node)} key={node.id}>
                          <span>{node.id === selectedEdge.from ? "기준" : "대상"}</span>
                          <strong title={nodeDisplayTitle(node)}>{nodeDisplayTitle(node)}</strong>
                        </button>
                      ))}
                    </div>
                  ) : (
                    <InspectorEmptyRow>이 관계의 양 끝 대상을 현재 지도에서 찾을 수 없습니다.</InspectorEmptyRow>
                  )
                ) : directEdges.length > 0 && relationTargetId ? (
                  <>
                    <div className="inspector-direct-relations">
                      {directEdges.slice(0, INSPECTOR_LIST_PREVIEW).map(renderDirectRelation)}
                    </div>
                    {directEdges.length > INSPECTOR_LIST_PREVIEW ? (
                      <details className="inspector-details" key={`${selectionKey}:relations`}>
                        <summary>{directEdges.length - INSPECTOR_LIST_PREVIEW}개 더 보기</summary>
                        <div className="inspector-direct-relations">
                          {directEdges.slice(INSPECTOR_LIST_PREVIEW).map(renderDirectRelation)}
                        </div>
                      </details>
                    ) : null}
                  </>
                ) : (
                  <InspectorEmptyRow>
                    {hasSelection
                      ? "이 대상에 바로 연결된 관계가 없습니다."
                      : "대상을 선택하면 한 단계 관계만 표시합니다."}
                  </InspectorEmptyRow>
                )}
              </InspectorSection>
            ) : null}

            <InspectorSection title="근거" count={evidenceItems.length}>
              {evidenceItems.length > 0 ? (
                <>
                  <div className="inspector-evidence-list">
                    {evidenceItems.slice(0, INSPECTOR_LIST_PREVIEW).map((item) => (
                      <span className={item.tone} key={item.key}>
                        {item.text}
                      </span>
                    ))}
                  </div>
                  {evidenceItems.length > INSPECTOR_LIST_PREVIEW ? (
                    <details className="inspector-details" key={`${selectionKey}:evidence`}>
                      <summary>{evidenceItems.length - INSPECTOR_LIST_PREVIEW}개 더 보기</summary>
                      <div className="inspector-evidence-list">
                        {evidenceItems.slice(INSPECTOR_LIST_PREVIEW).map((item) => (
                          <span className={item.tone} key={item.key}>
                            {item.text}
                          </span>
                        ))}
                      </div>
                    </details>
                  ) : null}
                </>
              ) : (
                <InspectorEmptyRow>
                  {hasSelection
                    ? "이 대상에 저장된 직접 근거가 없습니다."
                    : "대상을 선택하면 근거와 판정 수준을 표시합니다."}
                </InspectorEmptyRow>
              )}
              {hasCandidateRelation ? (
                <div className="inspector-candidate-note">
                  <TriangleAlert size={15} />
                  <span>
                    <strong>확정 관계가 아닙니다</strong>
                    <small>이름 단서 기반 후보이므로 소스에서 직접 사용 여부를 확인하세요.</small>
                  </span>
                </div>
              ) : null}
            </InspectorSection>

            <InspectorSection title="소스">
              {sourceCode ? (
                <>
                  <div className="inspector-source-summary">
                    <FileText size={14} />
                    <span>
                      <strong title={sourceCode.filePath ?? sourceCode.name}>
                        {compactPath(sourceCode.filePath) ??
                          routeDisplayName(sourceCode.name, codeRouteMethod(sourceCode))}
                      </strong>
                      <small>{sourceCode.line ? `${sourceCode.kind} · ${sourceCode.line}행` : sourceCode.kind}</small>
                    </span>
                  </div>
                  {sourceCode.filePath ? (
                    <SourceJump
                      key={`${workspaceControls.currentWorkspace?.id ?? "none"}:${sourceCode.id}`}
                      workspaceId={workspaceControls.currentWorkspace?.id ?? null}
                      code={sourceCode}
                    />
                  ) : null}
                </>
              ) : selectedTable || selectedColumn || selectedNode?.source === "db" ? (
                <div className="inspector-source-summary db">
                  <Type size={14} />
                  <span>
                    <strong>{dbProfileControls.activeProfile?.name ?? "DB 읽기 결과"}</strong>
                    <small>
                      {dbProfileControls.activeProfile
                        ? `${dbProfileSourceLabel(dbProfileControls.activeProfile.source)} · ${dbProfileControls.activeProfile.database ?? dbProfileControls.activeProfile.path ?? "연결 정보"}`
                        : "현재 인벤토리에 저장된 DB 구조"}
                    </small>
                  </span>
                </div>
              ) : selectedNode?.source === "code" && selectedNode.subtitle ? (
                <div className="inspector-source-summary">
                  <FileText size={14} />
                  <span>
                    <strong title={selectedNode.subtitle}>{compactPath(selectedNode.subtitle)}</strong>
                    <small>소스 위치 열기 정보 없음</small>
                  </span>
                </div>
              ) : (
                <InspectorEmptyRow>
                  {hasSelection
                    ? "이 대상에는 열 수 있는 소스 위치가 없습니다."
                    : "대상을 선택하면 파일 또는 DB 출처를 표시합니다."}
                </InspectorEmptyRow>
              )}
            </InspectorSection>
          </>
        ) : (
          /*
            With nothing selected the panel used to run the summary straight into
            the pinned footer, leaving 400px of blank white between them that
            read as a rendering fault rather than as an empty state.
          */
          <div className="inspector-idle" role="note">
            <MousePointer2 size={18} aria-hidden="true" />
            <strong>선택한 대상이 없습니다</strong>
            <span>캔버스의 박스나 왼쪽 목록에서 항목을 고르면 관계·근거·소스 위치가 여기에 표시됩니다.</span>
          </div>
        )}
      </div>

      <InspectorSection title="다음 확인">
        {suggestedCheck ? (
          <div className="inspector-next-check">
            {suggestedNode ? (
              <button type="button" onClick={() => visualMapControls.selectNode(suggestedNode)}>
                <GitBranch size={14} />
                <span>
                  <strong>{suggestedCheck.title}</strong>
                  <small>{suggestedCheck.detail}</small>
                </span>
              </button>
            ) : (
              <div>
                <GitBranch size={14} />
                <span>
                  <strong>{suggestedCheck.title}</strong>
                  <small>{suggestedCheck.detail}</small>
                </span>
              </div>
            )}
          </div>
        ) : nextAction ? (
          <button
            className={
              nextAction.primary
                ? "primary-action compact inspector-next-button"
                : "outline-action compact inspector-next-button"
            }
            type="button"
            onClick={nextAction.run}
            disabled={nextAction.disabled}
          >
            <MousePointer2 size={13} />
            <span>{nextAction.label}</span>
          </button>
        ) : nextCheckText ? (
          <div className="inspector-next-check">
            <div>
              <GitBranch size={14} />
              <span>
                <strong>{nextCheckText}</strong>
              </span>
            </div>
          </div>
        ) : (
          <InspectorEmptyRow>
            {hasSelection ? "추가로 제안할 확인 항목이 없습니다." : "대상을 선택하면 다음 확인 순서를 제안합니다."}
          </InspectorEmptyRow>
        )}
      </InspectorSection>
    </section>
  );
}

type InspectorIdentity = {
  kind: string;
  kindLabel: string;
  icon: ReactNode;
  title: string;
  id: string | null;
  detail: string | null;
};

/**
 * Names the selected target in the reader's terms: what it is, what it is
 * called, and the identifier they can search for. Kept free of judgement —
 * trust and evidence belong to the sections below.
 */
function inspectorIdentity({
  selectedEdge,
  selectedNode,
  selectedColumn,
  selectedTable,
  selectedCode,
  edgeFrom,
  edgeTo,
  apiMethod,
}: {
  selectedEdge: { id: string; from: string; to: string } | null | undefined;
  selectedNode: VisualNode | null | undefined;
  selectedColumn: { name: string } | null | undefined;
  selectedTable: DbInventoryTable | null | undefined;
  selectedCode: { id: string; name: string; kind: string; filePath?: string | null } | null | undefined;
  edgeFrom: string | null;
  edgeTo: string | null;
  apiMethod: string | null;
}): InspectorIdentity | null {
  if (selectedEdge) {
    return {
      kind: "edge",
      kindLabel: "관계",
      icon: <ArrowRight size={15} />,
      title: `${edgeFrom ?? selectedEdge.from} → ${edgeTo ?? selectedEdge.to}`,
      id: selectedEdge.id,
      detail: null,
    };
  }
  if (selectedColumn && selectedTable) {
    return {
      kind: "column",
      kindLabel: "컬럼",
      icon: <Type size={15} />,
      title: selectedColumn.name,
      id: `${dbInventoryTableKey(selectedTable)}.${selectedColumn.name}`,
      detail: `${dbInventoryTableKey(selectedTable)} 테이블의 컬럼`,
    };
  }
  if (selectedNode) {
    return {
      kind: selectedNode.kind,
      kindLabel: nodeKindLabel(selectedNode.kind),
      icon: <Info size={15} />,
      // A route without its verb is a different endpoint. The method is part of
      // the name, not decoration.
      title: routeDisplayName(selectedNode.title, apiMethod ?? routeMethodFromIdentity(selectedNode.id)),
      id: selectedNode.id,
      detail: selectedNode.subtitle?.split("|")[0] ?? null,
    };
  }
  if (selectedCode) {
    return {
      kind: "code",
      kindLabel: nodeKindLabel(selectedCode.kind),
      icon: <FileText size={15} />,
      title: selectedCode.name,
      id: selectedCode.id,
      detail: selectedCode.filePath ? compactPath(selectedCode.filePath) : null,
    };
  }
  if (selectedTable) {
    return {
      kind: "table",
      kindLabel: "테이블",
      icon: <FileText size={15} />,
      title: selectedTable.name,
      id: dbInventoryTableKey(selectedTable),
      detail: null,
    };
  }
  return null;
}
