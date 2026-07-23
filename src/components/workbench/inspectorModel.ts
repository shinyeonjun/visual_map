import {
  confidenceBadgeTone,
  confidenceLabel,
  confidenceReason as confidenceReasonLabel,
  normalizeConfidence,
} from "../../visual/confidence";
import type { DbProfileControls } from "../../types/controls";
import { codeRouteMethod, dbInventoryTableKey, routeDisplayName } from "../../types/workspace";
import type { CodeInventoryItem, DbInventoryColumn, DbInventoryTable } from "../../types/workspace";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  visualEdgeKindLabel as edgeKindLabel,
  visualEdgeTruthClass,
  visualMapModeLabel as modeLabel,
  visualNodeKindLabel as nodeKindLabel,
} from "../../visual/labels";
import { columnLabelFromNodeId, dbTableIdentityLabel, tableKeyFromDbNodeId } from "../../visual/nodeIds";

export type InspectorAnswer = {
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

export type InspectorAction = {
  label: string;
  run: () => void;
  primary?: boolean;
  disabled?: boolean;
};

export function inspectorAnswer({
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
  apiMethod,
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
  apiMethod?: string | null;
}): InspectorAnswer {
  if (edge) {
    const truthClass = visualEdgeTruthClass(edge);
    const isCandidate = truthClass === "candidate";
    const isInferred = truthClass === "inferred";
    const isStructural = truthClass === "structural";
    const isContainment = edge.kind === "contains" || edge.kind === "group_contains" || edge.kind.startsWith("structural_");
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
          : isContainment
            ? "프로젝트 구조를 읽기 쉽게 묶은 관계입니다. 직접 호출이나 DB 제약과 구분하세요."
          : isStructural
            ? hasCodeEndpoint
              ? "읽은 코드 구조의 관계입니다. 양끝 항목을 확인하세요."
              : "읽은 DB 구조/제약 관계입니다. 양끝 항목을 확인하세요."
          : hasCodeEndpoint
            ? "읽은 코드에서 확인된 1차 근거입니다."
            : "읽은 DB 구조에서 확인된 1차 근거입니다.",
      tone: isCandidate ? "candidate" : isInferred || isStructural ? "neutral" : "confirmed",
      metrics: [
        { label: "관계", value: edgeKindLabel(edge) },
        { label: "근거 수준", value: edgeTrustLabel(edge), tone: edgeTrustTone(edge) },
        { label: "근거 문장", value: hasEvidence ? `${edge.evidence.length}개` : "없음" },
      ],
      steps: [hasEvidence ? "근거 문장 확인" : "관계 구조와 출처 확인"],
      note: isCandidate ? "후보 근거는 이름 토큰 기반일 수 있어 확정 근거와 섞어 판단하면 안 됩니다." : undefined,
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
    const title = node.kind === "api"
      ? routeDisplayName(nodeDisplayTitle(node), apiMethod ?? (code ? codeRouteMethod(code) : null))
      : nodeDisplayTitle(node);
    return {
      kicker: `${nodeKindLabel(node.kind, node.source)} 근거`,
      title,
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
      title: routeDisplayName(code.name, codeRouteMethod(code)),
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
    kicker: "구조 영역 구성",
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
    note: "구조 영역 포함 관계는 패키지·DB 스키마 또는 보조 분류 기준이며 실제 호출이나 DB 제약을 뜻하지 않습니다.",
  };
}

function domainComposition(node: VisualNode): string {
  return node.kind === "group-domain"
    ? node.subtitle?.split("|")[0] ?? "구조 영역 구성"
    : node.subtitle ?? "";
}

export function compactPath(value?: string | null): string | null {
  const parts = value?.split(/[\\/]+/).filter(Boolean) ?? [];
  return parts.length ? parts.slice(-3).join("/") : null;
}

function connectionCounts(map: VisualMap | null, node: VisualNode): EdgeCounts {
  const edges = map?.edges.filter((edge) => edgeTouchesNode(edge, node)) ?? [];
  return edgeCounts(edges);
}

export function firstNodeRelationEdge(node: VisualNode, map: VisualMap | null): VisualEdge | null {
  const edges = map?.edges.filter((item) => edgeTouchesNode(item, node)) ?? [];
  return [...edges].sort((a, b) => relationPriority(a) - relationPriority(b))[0] ?? null;
}

export function edgeCopySummary(edge: VisualEdge, map: VisualMap | null): string {
  const evidence = edge.evidence[0]?.text ?? edgeKindLabel(edge);
  return `${relationshipSourceLabel(edge)} ${edgeKindLabel(edge)}: ${endpointLabel(edge.from, map)} → ${endpointLabel(edge.to, map)} · ${evidence}`;
}

function relationPriority(edge: VisualEdge): number {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "confirmed") return 0;
  if (truthClass === "structural") return 1;
  return truthClass === "candidate" ? 2 : 3;
}

function mapEdgeCounts(map: VisualMap): EdgeCounts {
  return edgeCounts(map.edges);
}

function edgeCounts(edges: VisualEdge[]): EdgeCounts {
  const counts: EdgeCounts = { confirmed: 0, typed: 0, inferred: 0, candidate: 0 };
  for (const edge of edges) {
    const truthClass = visualEdgeTruthClass(edge);
    if (truthClass === "candidate") counts.candidate += 1;
    else if (truthClass === "inferred") counts.inferred += 1;
    else if (truthClass === "structural") counts.typed += 1;
    else counts.confirmed += 1;
  }
  return counts;
}

function nodeRelationMetrics(counts: EdgeCounts): InspectorAnswer["metrics"] {
  const total = counts.confirmed + counts.typed + counts.inferred + counts.candidate;
  if (total === 0) {
    return [{ label: "관계", value: "0", tone: "gray" }];
  }
  return [
    ...(counts.confirmed > 0 ? [{ label: "확정", value: String(counts.confirmed), tone: "green" as const }] : []),
    ...(counts.typed > 0 ? [{ label: "구조", value: String(counts.typed), tone: "gray" as const }] : []),
    ...(counts.candidate > 0 ? [{ label: "후보", value: String(counts.candidate), tone: "amber" as const }] : []),
    ...(counts.inferred > 0 ? [{ label: "이름 단서", value: String(counts.inferred), tone: "gray" as const }] : []),
  ];
}

function nodeAnswerSentence(node: VisualNode, counts: EdgeCounts, map: VisualMap | null): string {
  if (node.kind === "group-domain") {
    return "이 구조 영역에 묶인 API, 코드, DB 항목을 읽는 순서대로 보여줍니다.";
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
  if (node.source === "db") {
    if (node.kind === "view") {
      return "이 뷰가 참조하는 테이블과 컬럼의 DB 근거입니다.";
    }
    if (node.kind === "trigger") {
      return "이 트리거가 등록된 테이블의 DB 근거입니다.";
    }
    if (node.kind === "routine") {
      return "이 DB 함수/프로시저가 참조하는 테이블과 컬럼의 DB 근거입니다.";
    }
    return "이 DB 객체와 연결된 구조 근거입니다.";
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
  if (node.source === "db") {
    if (node.kind === "trigger") {
      return ["등록된 테이블 확인"];
    }
    if (node.kind === "view" || node.kind === "routine") {
      return ["참조하는 테이블/컬럼 확인"];
    }
    return ["연결된 DB 구조 확인"];
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

export function relationshipSourceLabel(edge: VisualEdge): string {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") {
    return "후보";
  }
  if (truthClass === "inferred") {
    return "이름 단서";
  }
  if (truthClass === "structural") {
    return "구조";
  }
  return "확정";
}

export function edgeTrustLabel(edge: VisualEdge): string {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") {
    return `후보 ${confidenceLabel(edge.confidence) ?? "낮음"}`;
  }
  if (truthClass === "inferred") {
    return "이름 단서";
  }
  if (truthClass === "structural") {
    return "구조 근거";
  }
  return "근거 있음";
}

export function edgeTrustTone(edge: VisualEdge): "green" | "amber" | "gray" {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") {
    return confidenceBadgeTone(edge.confidence);
  }
  return truthClass === "confirmed" ? "green" : "gray";
}

export function edgeTrustReason(edge: VisualEdge): string {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") {
    return confidenceReasonLabel(edge.confidence);
  }
  if (truthClass === "inferred") {
    return "이름이 비슷해 이어 둔 후보 근거입니다.";
  }
  if (truthClass === "structural") {
    return edge.evidence[0]?.text ?? "프로젝트를 읽기 쉽게 묶은 구조 근거입니다.";
  }
  return edge.evidence[0]?.text ?? "확정 근거입니다.";
}

export function edgeEvidenceTone(edge: VisualEdge): "candidate" | "confirmed" | "neutral" {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") return "candidate";
  return truthClass === "confirmed" ? "confirmed" : "neutral";
}

export function endpointLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  if (!node) {
    return columnLabelFromNodeId(id) ??
      (id.startsWith("db:table:") ? dbTableIdentityLabel(id.slice("db:table:".length)) : id);
  }
  const title = nodeDisplayTitle(node);
  return node.kind === "column" ? title : node.subtitle ? `${title} (${node.subtitle})` : title;
}

function endpointTitleLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  return node
    ? nodeDisplayTitle(node)
    : columnLabelFromNodeId(id) ??
        (id.startsWith("db:table:") ? dbTableIdentityLabel(id.slice("db:table:".length)) : id);
}

export function nodeDisplayTitle(node: VisualNode): string {
  if (node.kind !== "column") {
    return node.title;
  }
  const tableKey = tableKeyFromDbNodeId(node.id);
  return tableKey ? `${dbTableIdentityLabel(tableKey)}.${node.title}` : node.title;
}

export function firstTableColumnAction(
  table: DbInventoryTable,
  dbProfileControls: DbProfileControls,
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
      dbProfileControls.openColumn(tableKey, column.name);
    },
    primary: true,
    disabled: dbProfileControls.busy,
  };
}

export function nodeSourceLabel(source: string): string {
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

export function relationshipReason(edge: VisualEdge): string {
  const truthClass = visualEdgeTruthClass(edge);
  if (truthClass === "candidate") {
    return edge.evidence[0]?.text ?? "이름이 비슷해 이어 둔 후보 근거입니다";
  }
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") {
    return edge.evidence[0]?.text ?? "DB 제약 구조입니다";
  }
  if (edge.kind === "db_index") {
    return edge.evidence[0]?.text ?? "DB 인덱스가 포함하는 컬럼 구조입니다";
  }
  if (edge.kind === "db_dependency") {
    return edge.evidence[0]?.text ?? "DB 뷰 또는 함수/프로시저가 참조하는 테이블이나 컬럼입니다";
  }
  if (edge.kind === "db_trigger") {
    return edge.evidence[0]?.text ?? "테이블에 등록된 DB 트리거입니다";
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
  if (truthClass === "structural") {
    return edge.evidence[0]?.text ?? "프로젝트를 읽기 위한 포함/구조 영역 관계입니다";
  }
  return edge.evidence[0]?.text ?? "이름과 구조 단서로 연결했습니다";
}

export function copyValuesForNode(node: VisualNode): Array<[string, string]> {
  if (node.kind === "group-domain") {
    return [["구조 영역", node.title], ["구성", domainComposition(node)], ["ID", node.id]];
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
  if (node.source === "db") {
    return [[nodeKindLabel(node.kind, node.source), node.title], ["ID", node.id]];
  }
  return [["심볼", node.title], ["경로", node.subtitle ?? ""], ["ID", node.id]];
}

export function nodeEvidenceSummary(
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
  const candidateEdges = relatedEdges.filter((edge) => visualEdgeTruthClass(edge) === "candidate");
  const inferredEdges = relatedEdges.filter((edge) => visualEdgeTruthClass(edge) === "inferred");
  const confirmedEdges = relatedEdges.filter((edge) => visualEdgeTruthClass(edge) === "confirmed");
  const typedEdges = relatedEdges.filter((edge) => visualEdgeTruthClass(edge) === "structural");
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
    connectionSummary: `확정 ${confirmedEdges.length} · 구조 ${typedEdges.length} · 후보 ${candidateEdges.length} · 이름 단서 ${inferredEdges.length}`,
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

export function nodeHasCodeRelation(node: VisualNode, map: VisualMap | null): boolean {
  return Boolean(
    map?.edges.some((edge) => edgeTouchesNode(edge, node) && (edge.from.startsWith("code:") || edge.to.startsWith("code:"))),
  );
}

function edgeHasCodeEndpoint(edge: VisualEdge): boolean {
  return edge.from.startsWith("code:")
    || edge.to.startsWith("code:")
    || edge.kind.endsWith("code_call")
    || edge.kind.endsWith("code_handle");
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

export function columnImpactSummary(node: VisualNode, map: VisualMap | null): {
  directCount: number;
  candidateCount: number;
  constraints: string;
} {
  const connectedEdges = map?.edges.filter((edge) => edge.from === node.id || edge.to === node.id) ?? [];
  const directCount = connectedEdges.filter(
    (edge) => !edge.kind.startsWith("candidate") && edge.kind !== "contains" && edge.kind !== "group_contains",
  ).length;
  const candidateCount = connectedEdges.filter((edge) => edge.kind.startsWith("candidate")).length;
  const fkCount = connectedEdges.filter((edge) => edge.kind === "db_fk" || edge.kind === "db_constraint").length;
  const constraints = fkCount > 0 ? `FK 관계 ${fkCount}개` : "-";
  return { directCount, candidateCount, constraints };
}
