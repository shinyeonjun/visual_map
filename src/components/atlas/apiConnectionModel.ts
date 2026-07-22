import type {
  ApiReadingAnswer,
  ApiReadingStep,
  ImpactReviewItem,
  VisualEdge,
  VisualMap,
  VisualNode,
} from "../../types/visual-map";

type DiagramItem = {
  item: ApiReadingStep | ImpactReviewItem;
  node: VisualNode;
};

export type ApiConnectionModel = {
  primaryPath: DiagramItem[];
  primaryEdges: VisualEdge[];
  primaryCandidate: (DiagramItem & { edge: VisualEdge }) | null;
  additionalEdges: VisualEdge[];
  gap: ImpactReviewItem | null;
};

export function buildApiConnectionModel(answer: ApiReadingAnswer, map: VisualMap): ApiConnectionModel {
  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const stepsByNodeId = new Map(
    answer.steps.flatMap((step) => (step.nodeId ? [[step.nodeId, step] as const] : [])),
  );
  const confirmedEdges = map.edges.filter(isConfirmedApiEdge);
  const candidateEdges = map.edges.filter(isCandidateEdge);
  const startId = stepsByNodeId.has(map.focus)
    ? map.focus
    : answer.steps.find((step) => step.lane === "route")?.nodeId ?? answer.steps[0]?.nodeId ?? null;
  const pathIds = startId ? choosePrimaryPath(startId, confirmedEdges, stepsByNodeId, candidateEdges) : [];
  const primaryPath = pathIds.flatMap((nodeId) => {
    const item = stepsByNodeId.get(nodeId);
    const node = nodesById.get(nodeId);
    return item && node ? [{ item, node }] : [];
  });
  const primaryEdges = pathIds.slice(0, -1).flatMap((from, index) => {
    const to = pathIds[index + 1];
    const edge = confirmedEdges.find((candidate) => candidate.from === from && candidate.to === to);
    return edge ? [edge] : [];
  });
  const pathIndex = new Map(pathIds.map((nodeId, index) => [nodeId, index]));
  const candidatesByNodeId = new Map(
    answer.dbCandidates.flatMap((item) => (item.nodeId ? [[item.nodeId, item] as const] : [])),
  );
  const primaryCandidateEdge = [...candidateEdges]
    .filter((edge) => pathIndex.has(edge.from) && candidatesByNodeId.has(edge.to) && nodesById.has(edge.to))
    .sort((left, right) => {
      const leftDepth = pathIndex.get(left.from) ?? -1;
      const rightDepth = pathIndex.get(right.from) ?? -1;
      const leftRank = candidatesByNodeId.get(left.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      const rightRank = candidatesByNodeId.get(right.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      return rightDepth - leftDepth || leftRank - rightRank || left.id.localeCompare(right.id);
    })[0] ?? null;
  const primaryCandidate = primaryCandidateEdge
    ? {
        item: candidatesByNodeId.get(primaryCandidateEdge.to)!,
        node: nodesById.get(primaryCandidateEdge.to)!,
        edge: primaryCandidateEdge,
      }
    : null;
  const usedEdgeIds = new Set([
    ...primaryEdges.map((edge) => edge.id),
    ...(primaryCandidate ? [primaryCandidate.edge.id] : []),
  ]);
  const additionalEdges = map.edges.filter((edge) => !usedEdgeIds.has(edge.id));
  const gap = answer.unknowns.find((item) => item.kind === "handler-gap") ?? null;

  return { primaryPath, primaryEdges, primaryCandidate, additionalEdges, gap };
}

function choosePrimaryPath(
  startId: string,
  edges: VisualEdge[],
  stepsByNodeId: Map<string, ApiReadingStep>,
  candidateEdges: VisualEdge[],
): string[] {
  const outgoing = new Map<string, VisualEdge[]>();
  for (const edge of edges) {
    if (!stepsByNodeId.has(edge.from) || !stepsByNodeId.has(edge.to)) continue;
    const bucket = outgoing.get(edge.from) ?? [];
    bucket.push(edge);
    outgoing.set(edge.from, bucket);
  }
  for (const bucket of outgoing.values()) {
    bucket.sort((left, right) => {
      const leftRank = stepsByNodeId.get(left.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      const rightRank = stepsByNodeId.get(right.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      return leftRank - rightRank || left.id.localeCompare(right.id);
    });
  }

  const candidateSources = new Set(candidateEdges.map((edge) => edge.from));
  const queue: string[][] = [[startId]];
  let best = queue[0];
  let explored = 0;
  while (queue.length > 0 && explored < 2_048) {
    const path = queue.shift()!;
    explored += 1;
    if (isBetterPath(path, best, candidateSources, stepsByNodeId)) best = path;
    if (path.length >= 8) continue;
    const last = path[path.length - 1];
    for (const edge of outgoing.get(last) ?? []) {
      if (!path.includes(edge.to)) queue.push([...path, edge.to]);
    }
  }
  return best;
}

function isBetterPath(
  candidate: string[],
  current: string[],
  candidateSources: Set<string>,
  stepsByNodeId: Map<string, ApiReadingStep>,
): boolean {
  const candidateScore = pathScore(candidate, candidateSources);
  const currentScore = pathScore(current, candidateSources);
  if (candidateScore !== currentScore) return candidateScore > currentScore;
  const candidateRanks = candidate.map((id) => stepsByNodeId.get(id)?.rank ?? Number.MAX_SAFE_INTEGER).join(":");
  const currentRanks = current.map((id) => stepsByNodeId.get(id)?.rank ?? Number.MAX_SAFE_INTEGER).join(":");
  return candidateRanks < currentRanks;
}

function pathScore(path: string[], candidateSources: Set<string>): number {
  const lastHasCandidate = candidateSources.has(path[path.length - 1]);
  const containsCandidateSource = path.some((id) => candidateSources.has(id));
  return (containsCandidateSource ? 1_000_000 : 0) + path.length * 10_000 + (lastHasCandidate ? 500 : 0);
}

function isConfirmedApiEdge(edge: VisualEdge): boolean {
  return edge.kind === "code_handle" || edge.kind === "code_call";
}

function isCandidateEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate");
}
