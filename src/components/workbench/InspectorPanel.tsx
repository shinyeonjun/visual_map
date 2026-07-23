import { ArrowRight, Copy, FileText, GitBranch, Info, LoaderCircle, MousePointer2, TriangleAlert, Type, X } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { dbProfileWorkStarted } from "../../types/controls";
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
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  visualEdgeKindLabel as edgeKindLabel,
  visualEdgeTruthClass,
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

const INSPECTOR_EVIDENCE_LABELS: Record<string, string> = {
  "route-mount": "경로 근거",
  "route-source": "소스 근거",
  "route-binding": "라우트 연결",
  "code-call": "호출 관계",
  "code-handle": "라우트 연결",
  "code-db-read": "DB 조회",
  "code-db-write": "DB 변경",
  "code-db-column": "컬럼 사용",
  "db-constraint": "DB 제약",
  "db-dependency": "DB 의존",
  "db-trigger": "DB 트리거",
  "snapshot-link": "확정 연결",
  "engine-node": "코드 항목",
  "engine-edge": "관계 근거",
  "engine-confidence": "신뢰 수준",
  "engine-confidence-score": "신뢰 점수",
  "engine-strategy": "분석 방식",
  "engine-callee": "호출 표현",
  "candidate-source": "후보 출처",
  "static-sql": "정적 SQL",
};

const INSPECTOR_CONFIDENCE_LABELS: Record<string, string> = {
  high: "높음",
  medium: "중간",
  low: "낮음",
  unknown: "확인 필요",
};

const INSPECTOR_STRATEGY_LABELS: Record<string, string> = {
  lsp_direct: "LSP 직접 확인",
  lsp_implicit_this: "LSP 현재 객체 추적",
  lsp_type_dispatch: "LSP 타입 추적",
  lsp_virtual_dispatch: "LSP 가상 호출 추적",
  import_map: "import 연결 확인",
  import_map_suffix: "import 경로 추적",
  same_module: "같은 모듈 확인",
  service_pattern: "프레임워크 패턴 확인",
  unique_name: "고유 이름 일치",
};

const INSPECTOR_ENGINE_EDGE_LABELS: Record<string, string> = {
  "codebase-memory CALLS": "코드 엔진에서 호출 관계를 확인했습니다.",
  "codebase-memory HANDLES: upstream handler→route was normalized to product route→handler":
    "코드 엔진의 핸들러→라우트 관계를 제품의 라우트→핸들러 읽기 방향으로 정규화했습니다.",
};

export function InspectorPanel({
  onClose,
  title = "선택한 대상",
  variant = "full",
  showDbSetup,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  onClose?: () => void;
  title?: string;
  variant?: "full" | "answer";
  showDbSetup?: () => void;
  showWorkspaceSetup?: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  if (visualMapControls.loading && !visualMapControls.currentMap) {
    return <InspectorTransitionState mode={visualMapControls.mode} onClose={onClose} title={title} />;
  }

  const selectedEdge = visualMapControls.selectedEdge;
  const visibleMode = visualMapControls.currentMap?.mode ?? visualMapControls.mode;
  const apiReading = visibleMode === "api-flow" ? visualMapControls.currentMap?.apiReading ?? null : null;
  const analysisFocusId = visibleMode === "composition"
    ? visualMapControls.selectedNode?.id ?? ""
    : visualMapControls.loading && visualMapControls.currentMap
      ? visualMapControls.currentMap.focus
      : visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? "";
  const selectedNode = visualMapControls.selectedNode ?? (
    !selectedEdge && apiReading
      ? visualMapControls.currentMap?.nodes.find((node) => node.id === visualMapControls.currentMap?.focus) ?? null
      : null
  );
  const selectedEdgeFrom = selectedEdge ? endpointLabel(selectedEdge.from, visualMapControls.currentMap) : null;
  const selectedEdgeTo = selectedEdge ? endpointLabel(selectedEdge.to, visualMapControls.currentMap) : null;
  const columnImpact = selectedNode?.kind === "column" ? columnImpactSummary(selectedNode, visualMapControls.currentMap) : null;
  const selectedNodeHasCodeRelation = selectedNode ? nodeHasCodeRelation(selectedNode, visualMapControls.currentMap) : false;
  const nodeEvidence = selectedNode ? nodeEvidenceSummary(selectedNode, visualMapControls.currentMap) : null;
  const focusedNodeId = selectedNode?.id ?? analysisFocusId;
  const focusedCodeId = selectedNode?.source === "code"
    ? selectedNode.id.replace(/^code:/, "")
    : !selectedNode && focusedNodeId.startsWith("code:")
      ? focusedNodeId.replace(/^code:/, "")
      : null;
  const focusedMapNode = visualMapControls.currentMap?.nodes.find((node) => node.id === focusedNodeId) ?? null;
  const selectedCode = codeInventoryItemById(workspaceControls.codeInventory, focusedCodeId)
    ?? codeInventoryItemFromNode(selectedNode ?? focusedMapNode);
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
  const focusedColumn = selectedNode?.kind === "column"
    ? columnRefFromNodeId(selectedNode.id)
    : !selectedNode
      ? columnRefFromNodeId(focusedNodeId)
      : null;
  const selectedNodeTableKey = selectedNode
    ? tableKeyFromDbNodeId(selectedNode.id)
    : focusedColumn?.tableKey ?? tableKeyFromDbNodeId(focusedNodeId);
  const useSelectedTable =
    focusedNodeId.startsWith("db:") ||
    visibleMode === "table-usage" ||
    visibleMode === "column-impact";
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
  const relationTargetId = selectedNode?.id ?? (hasSelection ? focusedNodeId || null : null);
  const directEdges = relationTargetId
    ? visualMapControls.currentMap?.edges.filter((edge) => edge.from === relationTargetId || edge.to === relationTargetId) ?? []
    : [];
  const directEvidence = uniqueInspectorEvidence(directEdges.flatMap((edge) => edge.evidence.map((item) => ({
    key: `${item.kind}:${item.text}`,
    text: inspectorEvidenceText(item.kind, item.text),
    tone: edgeEvidenceTone(edge),
  }))));
  const reviewBoardEvidence = uniqueInspectorEvidence(
    (visualMapControls.currentMap?.reviewBoard?.lanes.find((lane) => lane.id === "direct")?.items ?? [])
      .filter((item) => !selectedNode || !item.nodeId || item.nodeId === selectedNode.id)
      .map((item) => ({
        key: item.id,
        text: `${item.title} · ${item.detail}`,
        tone: item.truthClass === "confirmed" ? "confirmed" : "neutral",
      })),
  );
  const apiStep = selectedNode
    ? apiReading?.steps.find((step) => step.nodeId === selectedNode.id) ?? null
    : null;
  const apiStepEvidence = apiStep
    ? apiStep.evidence.map((item) => ({
        key: `${item.kind}:${item.text}`,
        text: inspectorEvidenceText(item.kind, item.text),
        tone: apiStep.truthClass === "candidate"
          ? "candidate"
          : apiStep.truthClass === "confirmed"
            ? "confirmed"
            : "neutral",
      }))
    : [];
  const nodeEvidenceItems = uniqueInspectorEvidence([
    ...(nodeEvidence?.evidence ?? []),
    ...apiStepEvidence,
  ]);
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
    ? [selectedEdge.from, selectedEdge.to]
        .map((id) => visualMapControls.currentMap?.nodes.find((node) => node.id === id) ?? null)
        .find((node) => node?.source === "code") ?? null
    : null;
  const sourceCode = edgeCodeNode
    ? codeInventoryItemById(workspaceControls.codeInventory, edgeCodeNode.id.replace(/^code:/, ""))
      ?? codeInventoryItemFromNode(edgeCodeNode)
    : selectedCode;
  const apiNextCheck = apiReading
    ? selectedNode
      ? apiReading.recommendedChecks.find((item) => item.nodeId === selectedNode.id) ?? apiReading.recommendedChecks[0] ?? null
      : apiReading.recommendedChecks[0] ?? null
    : null;
  const reviewNextCheck = visualMapControls.currentMap?.reviewBoard?.lanes
    .find((lane) => lane.id === "checks")
    ?.items[0] ?? null;
  const suggestedCheck = apiNextCheck ?? reviewNextCheck;
  const suggestedNode = suggestedCheck?.nodeId
    ? visualMapControls.currentMap?.nodes.find((node) => node.id === suggestedCheck.nodeId) ?? null
    : null;
  const nextAction = selectionAction ?? emptyAction;
  const selectedEdgeNodes = selectedEdge
    ? [selectedEdge.from, selectedEdge.to]
        .map((id) => visualMapControls.currentMap?.nodes.find((node) => node.id === id) ?? null)
        .filter((node): node is VisualNode => Boolean(node))
    : [];
  const hasCandidateRelation = selectedEdge
    ? requiresReview(selectedEdge)
    : directEdges.some(requiresReview);
  const nextCheckText = answer.steps[1] ?? null;
  const selectionKey = selectedEdge?.id
    ?? selectedNode?.id
    ?? selectedCode?.id
    ?? (selectedColumn && selectedTable ? `${dbInventoryTableKey(selectedTable)}.${selectedColumn.name}` : null)
    ?? (selectedTable ? dbInventoryTableKey(selectedTable) : "none");

  return (
    <section
      className={`side-card inspector${variant === "answer" ? " answer-inspector" : ""}${visualMapControls.loading ? " is-refreshing" : ""}`}
      aria-busy={visualMapControls.loading}
    >
      <div className="panel-header">
        <Info size={16} />
        <h2>{title}</h2>
        {onClose ? <button className="inspector-close" type="button" onClick={onClose} aria-label="선택 해제" title="선택 해제"><X size={15} /></button> : null}
      </div>
      {visualMapControls.loading ? <InspectorUpdating /> : null}
      <div className="inspector-scroll-body">
        <InspectorSection title={variant === "answer" ? "선택" : "요약"}>
        <AnswerSummary answer={answer} />
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
              <CopyRow values={[["관계", edgeCopySummary(selectedEdge, visualMapControls.currentMap)], ["기준", selectedEdge.from], ["연결 대상", selectedEdge.to]]} />
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
            </div>
          </details>
        )}
        </InspectorSection>

        {hasSelection ? <>
        {variant === "full" ? <InspectorSection title="바로 연결" count={selectedEdge ? selectedEdgeNodes.length : directEdges.length}>
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
          <div className="inspector-direct-relations">
            {directEdges.map((edge) => {
              const outbound = edge.from === relationTargetId;
              const otherId = outbound ? edge.to : edge.from;
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
                    <small title={endpointLabel(otherId, visualMapControls.currentMap)}>
                      {endpointLabel(otherId, visualMapControls.currentMap)}
                    </small>
                  </span>
                </button>
              );
            })}
          </div>
        ) : (
          <InspectorEmptyRow>
            {hasSelection ? "이 대상에 바로 연결된 관계가 없습니다." : "대상을 선택하면 한 단계 관계만 표시합니다."}
          </InspectorEmptyRow>
        )}
        </InspectorSection> : null}

        <InspectorSection title="근거" count={evidenceItems.length}>
        {evidenceItems.length > 0 ? (
          <>
            <div className="inspector-evidence-list">
              {evidenceItems.slice(0, 6).map((item) => (
                <span className={item.tone} key={item.key}>{item.text}</span>
              ))}
            </div>
            {evidenceItems.length > 6 ? (
              <details className="inspector-details" key={`${selectionKey}:evidence`}>
                <summary>{evidenceItems.length - 6}개 더 보기</summary>
                <div className="inspector-evidence-list">
                  {evidenceItems.slice(6).map((item) => (
                    <span className={item.tone} key={item.key}>{item.text}</span>
                  ))}
                </div>
              </details>
            ) : null}
          </>
        ) : (
          <InspectorEmptyRow>
            {hasSelection ? "이 대상에 저장된 직접 근거가 없습니다." : "대상을 선택하면 근거와 판정 수준을 표시합니다."}
          </InspectorEmptyRow>
        )}
        {hasCandidateRelation ? (
          <div className="inspector-candidate-note">
            <TriangleAlert size={15} />
            <span><strong>확정 관계가 아닙니다</strong><small>이름 단서 기반 후보이므로 소스에서 직접 사용 여부를 확인하세요.</small></span>
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
                  {compactPath(sourceCode.filePath) ?? routeDisplayName(sourceCode.name, codeRouteMethod(sourceCode))}
                </strong>
                <small>{sourceCode.line ? `${sourceCode.kind} · ${sourceCode.line}행` : sourceCode.kind}</small>
              </span>
            </div>
            {sourceCode.filePath ? (
              <SourceJumpRow
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
            <span><strong title={selectedNode.subtitle}>{compactPath(selectedNode.subtitle)}</strong><small>소스 위치 열기 정보 없음</small></span>
          </div>
        ) : (
          <InspectorEmptyRow>
            {hasSelection ? "이 대상에는 열 수 있는 소스 위치가 없습니다." : "대상을 선택하면 파일 또는 DB 출처를 표시합니다."}
          </InspectorEmptyRow>
        )}
        </InspectorSection>
        </> : null}
      </div>

      <InspectorSection title="다음 확인">
        {suggestedCheck ? (
          <div className="inspector-next-check">
            {suggestedNode ? (
              <button type="button" onClick={() => visualMapControls.selectNode(suggestedNode)}>
                <GitBranch size={14} />
                <span><strong>{suggestedCheck.title}</strong><small>{suggestedCheck.detail}</small></span>
              </button>
            ) : (
              <div><GitBranch size={14} /><span><strong>{suggestedCheck.title}</strong><small>{suggestedCheck.detail}</small></span></div>
            )}
          </div>
        ) : nextAction ? (
          <button
            className={nextAction.primary ? "primary-action compact inspector-next-button" : "outline-action compact inspector-next-button"}
            type="button"
            onClick={nextAction.run}
            disabled={nextAction.disabled}
          >
            <MousePointer2 size={13} />
            <span>{nextAction.label}</span>
          </button>
        ) : nextCheckText ? (
          <div className="inspector-next-check"><div><GitBranch size={14} /><span><strong>{nextCheckText}</strong></span></div></div>
        ) : (
          <InspectorEmptyRow>
            {hasSelection ? "추가로 제안할 확인 항목이 없습니다." : "대상을 선택하면 다음 확인 순서를 제안합니다."}
          </InspectorEmptyRow>
        )}
      </InspectorSection>
    </section>
  );
}

function InspectorTransitionState({ mode, onClose, title }: { mode: string; onClose?: () => void; title: string }) {
  const subject = inspectorTransitionSubject(mode);
  return (
    <section className="side-card inspector is-transitioning" aria-busy="true">
      <div className="panel-header">
        <Info size={16} />
        <h2>{title}</h2>
        {onClose ? <button className="inspector-close" type="button" onClick={onClose} aria-label="선택 해제" title="선택 해제"><X size={15} /></button> : null}
      </div>
      <div className="evidence-transition-state" role="status" aria-live="polite">
        <div className="evidence-transition-summary">
          <span>현재 보기</span>
          <strong>{subject}</strong>
          <small>새 화면과 일치하는 근거를 구성하고 있습니다.</small>
        </div>
        {[
          ["요약", 2],
          ["바로 연결", 3],
          ["근거", 2],
          ["소스", 2],
          ["다음 확인", 1],
        ].map(([label, count]) => (
          <section className="evidence-transition-section" key={label}>
            <header><strong>{label}</strong><span /></header>
            {Array.from({ length: Number(count) }, (_, index) => (
              <i className={index === 0 ? "wide" : ""} aria-hidden="true" key={index} />
            ))}
          </section>
        ))}
        <footer>
          <LoaderCircle className="spin" size={14} />
          이전 화면의 값은 섞지 않습니다
        </footer>
      </div>
    </section>
  );
}

function InspectorUpdating() {
  return (
    <div className="inspector-update-indicator" role="status" aria-live="polite">
      <LoaderCircle className="spin" size={13} />
      새 대상 분석 중 · 이전 근거 표시
    </div>
  );
}

function inspectorTransitionSubject(mode: string): string {
  if (mode === "api-flow") return "API 읽기 경로";
  if (mode === "table-usage") return "테이블 사용처";
  if (mode === "column-impact") return "컬럼 변경 영향";
  if (mode === "search-focus") return "코드 연결";
  return "전체 구조";
}

function apiEdgeLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  if (visualEdgeTruthClass(edge) === "candidate") return "DB 후보";
  return edgeKindLabel(edge);
}

function requiresReview(edge: VisualEdge): boolean {
  const truthClass = visualEdgeTruthClass(edge);
  return truthClass === "candidate" || truthClass === "inferred";
}

function uniqueInspectorEvidence<T extends { key: string; text: string }>(items: T[]): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = item.text;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function inspectorEvidenceText(kind: string, text: string): string {
  const value = kind === "engine-confidence"
    ? INSPECTOR_CONFIDENCE_LABELS[text] ?? text
    : kind === "engine-strategy"
      ? INSPECTOR_STRATEGY_LABELS[text] ?? text
      : kind === "engine-edge"
        ? INSPECTOR_ENGINE_EDGE_LABELS[text] ?? text
        : text;
  return `${INSPECTOR_EVIDENCE_LABELS[kind] ?? "근거"}: ${value}`;
}

function InspectorSection({
  title,
  count,
  children,
}: {
  title: string;
  count?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="inspector-section">
      <header>
        <strong>{title}</strong>
        {count !== undefined ? <span>{count}</span> : null}
      </header>
      {children}
    </section>
  );
}

function InspectorEmptyRow({ children }: { children: ReactNode }) {
  return <p className="inspector-empty-row">{children}</p>;
}

function AnswerSummary({ answer }: { answer: InspectorAnswer }) {
  const [firstStep] = answer.steps;

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
      {answer.note && <small>{answer.note}</small>}
    </div>
  );
}

function answerVerdict(answer: InspectorAnswer): string {
  if (answer.tone === "confirmed") {
    return "확정";
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

function codeInventoryItemFromNode(node: VisualNode | null): CodeInventoryItem | null {
  if (node?.source !== "code" || !node.location?.path) {
    return null;
  }
  return {
    id: node.id.replace(/^code:/, ""),
    kind: node.kind,
    name: node.title,
    filePath: node.location.path,
    line: node.location.line ?? null,
    column: node.location.column ?? null,
    endLine: node.location.endLine ?? null,
    endColumn: node.location.endColumn ?? null,
    detail: null,
  };
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
    if (!workspaceControls.canIndexCode) {
      return {
        label: "DB 정보 입력",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return { label: "코드 읽기", run: workspaceControls.indexCodeRepository, primary: true, disabled: workspaceControls.busy };
  }

  if (!dbProfileControls.inventory) {
    if (!dbProfileControls.activeProfile) {
      return dbProfileControls.canSaveProfile
        ? { label: "DB 연결 저장", run: dbProfileControls.saveProfile, primary: true, disabled: dbProfileControls.busy }
        : { label: "DB 정보 입력", run: showDbSetup, disabled: dbProfileControls.busy };
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
  return `첫 ${nodeKindLabel(node.kind, node.source)} 보기`;
}

function hasSearchableInventory(workspaceControls: WorkspaceControls, dbProfileControls: DbProfileControls): boolean {
  return codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
}
