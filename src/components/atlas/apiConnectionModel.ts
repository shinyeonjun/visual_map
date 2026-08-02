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
  primaryDatabase: (DiagramItem & { edge: VisualEdge }) | null;
  additionalEdges: VisualEdge[];
  collapsedEdges: VisualEdge[];
  gap: ImpactReviewItem | null;
};

const VISIBLE_ADDITIONAL_EDGE_LIMIT = 6;

export function buildApiConnectionModel(answer: ApiReadingAnswer, map: VisualMap): ApiConnectionModel {
  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const stepsByNodeId = new Map(
    answer.steps.flatMap((step) => (step.nodeId ? [[step.nodeId, step] as const] : [])),
  );
  const confirmedEdges = map.edges.filter(isConfirmedApiEdge);
  const candidateEdges = map.edges.filter(isCandidateApiEdge);
  const directDatabaseEdges = map.edges.filter(isDirectDatabaseEdge);
  const databaseEdges = [...directDatabaseEdges, ...candidateEdges];
  const startId = stepsByNodeId.has(map.focus)
    ? map.focus
    : answer.steps.find((step) => step.lane === "route")?.nodeId ?? answer.steps[0]?.nodeId ?? null;
  const pathIds = startId ? choosePrimaryPath(startId, confirmedEdges, stepsByNodeId, databaseEdges) : [];
  const primaryPath: DiagramItem[] = pathIds.flatMap((nodeId) => {
    const item = stepsByNodeId.get(nodeId);
    const node = nodesById.get(nodeId);
    return item && node ? [{ item, node }] : [];
  });
  const primaryEdges = pathIds.slice(0, -1).flatMap((from, index) => {
    const to = pathIds[index + 1];
    const edge = confirmedEdges.find((candidate) => candidate.from === from && candidate.to === to);
    return edge ? [edge] : [];
  });
  const clientRequestEdge = [...map.edges]
    .filter((edge) => edge.kind === "client_request" && edge.to === primaryPath[0]?.node.id)
    .sort((left, right) => Number(isCandidateApiEdge(left)) - Number(isCandidateApiEdge(right)) || left.id.localeCompare(right.id))[0];
  const clientRequestItem = clientRequestEdge
    ? answer.clientRequests?.find((item) => item.nodeId === clientRequestEdge.from)
    : undefined;
  if (clientRequestEdge && clientRequestItem) {
    const node = nodesById.get(clientRequestEdge.from);
    if (node) {
      primaryPath.unshift({ item: clientRequestItem, node });
      primaryEdges.unshift(clientRequestEdge);
    }
  }
  const pathIndex = new Map(pathIds.map((nodeId, index) => [nodeId, index]));
  const databaseItemsByNodeId = new Map(
    [...(answer.dbRelations ?? []), ...answer.dbCandidates]
      .flatMap((item) => (item.nodeId ? [[item.nodeId, item] as const] : [])),
  );
  const primaryDatabaseEdge = [...databaseEdges]
    .filter((edge) => pathIndex.has(edge.from) && databaseItemsByNodeId.has(edge.to) && nodesById.has(edge.to))
    .sort((left, right) => {
      const truthOrder = Number(isCandidateApiEdge(left)) - Number(isCandidateApiEdge(right));
      const leftDepth = pathIndex.get(left.from) ?? -1;
      const rightDepth = pathIndex.get(right.from) ?? -1;
      const leftRank = databaseItemsByNodeId.get(left.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      const rightRank = databaseItemsByNodeId.get(right.to)?.rank ?? Number.MAX_SAFE_INTEGER;
      return truthOrder || rightDepth - leftDepth || leftRank - rightRank || left.id.localeCompare(right.id);
    })[0] ?? null;
  const primaryDatabase = primaryDatabaseEdge
    ? {
        item: databaseItemsByNodeId.get(primaryDatabaseEdge.to)!,
        node: nodesById.get(primaryDatabaseEdge.to)!,
        edge: primaryDatabaseEdge,
      }
    : null;
  const usedEdgeIds = new Set([
    ...primaryEdges.map((edge) => edge.id),
    ...(primaryDatabase ? [primaryDatabase.edge.id] : []),
  ]);
  const remainingEdges = map.edges.filter((edge) => !usedEdgeIds.has(edge.id));
  const primaryNodeIds = new Set([
    ...primaryPath.map(({ node }) => node.id),
    ...(primaryDatabase ? [primaryDatabase.node.id] : []),
  ]);
  const rankedRemainingEdges = [...remainingEdges].sort((left, right) => {
    const leftPrimary = Number(primaryNodeIds.has(left.from));
    const rightPrimary = Number(primaryNodeIds.has(right.from));
    return rightPrimary - leftPrimary
      || Number(isCandidateApiEdge(left)) - Number(isCandidateApiEdge(right))
      || left.id.localeCompare(right.id);
  });
  const additionalEdges = rankedRemainingEdges.slice(0, VISIBLE_ADDITIONAL_EDGE_LIMIT);
  const collapsedEdges = rankedRemainingEdges.slice(VISIBLE_ADDITIONAL_EDGE_LIMIT);
  const gap = answer.unknowns.find((item) => item.kind === "handler-gap") ?? null;

  return { primaryPath, primaryEdges, primaryDatabase, additionalEdges, collapsedEdges, gap };
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

export function isConfirmedApiEdge(edge: VisualEdge): boolean {
  return edge.kind === "code_handle" || edge.kind === "code_call";
}

export function isCandidateApiEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate") || edge.confidence === "candidate";
}

function isDirectDatabaseEdge(edge: VisualEdge): boolean {
  return edge.kind === "code_db_read" || edge.kind === "code_db_write";
}

export function isDatabaseEdge(edge: VisualEdge): boolean {
  return isCandidateApiEdge(edge) || isDirectDatabaseEdge(edge);
}
