export type VisualMap = {
  id: string;
  workspaceId: string;
  mode: string;
  focus: string;
  nodes: VisualNode[];
  edges: VisualEdge[];
  warnings: string[];
  reviewBoard?: ImpactReviewBoard | null;
  apiReading?: ApiReadingAnswer | null;
};

export type ApiReadingAnswer = {
  subject: string;
  steps: ApiReadingStep[];
  dbCandidates: ImpactReviewItem[];
  unknowns: ImpactReviewItem[];
  recommendedChecks: ImpactReviewItem[];
  hiddenBranches: number;
  hiddenBranchesIsLowerBound?: boolean;
  truncated: boolean;
  truncationReason?: string | null;
};

export type ApiReadingStep = ImpactReviewItem & {
  depth: number;
  lane: "route" | "handler" | "service-function" | "repository-query" | string;
  incomingEvidence: { kind: string; text: string }[];
};

export type ImpactReviewBoard = {
  subject: string;
  scope: string;
  lanes: ImpactReviewLane[];
  markdownSummary: string;
};

export type ImpactReviewLane = {
  id: "direct" | "candidates" | "unknowns" | "checks" | string;
  order: number;
  title: string;
  description: string;
  tone: "confirmed" | "candidate" | "unknown" | "action" | string;
  total: number;
  hidden: number;
  emptyMessage: string;
  items: ImpactReviewItem[];
};

export type ImpactReviewItem = {
  id: string;
  nodeId?: string | null;
  kind: string;
  title: string;
  detail: string;
  truthClass: "confirmed" | "structural" | "candidate" | "unknown" | "action" | string;
  confidence?: string | null;
  rank: number;
  evidence: { kind: string; text: string }[];
  location?: SourceLocation | null;
};

export type VisualNode = {
  id: string;
  kind: string;
  title: string;
  subtitle?: string | null;
  layer: string;
  source: string;
};

export type VisualEdge = {
  id: string;
  from: string;
  to: string;
  kind: string;
  confidence?: string | null;
  evidence: { kind: string; text: string }[];
};

export type InventoryItem = {
  id: string;
  kind: string;
  name: string;
  layer: string;
  source: string;
  parentId?: string | null;
  path?: string | null;
  qualifiedName?: string | null;
  engineLabel?: string | null;
  projectId?: string | null;
  groupId?: string | null;
  location?: SourceLocation | null;
  isPrimaryKey?: boolean;
  isForeignKey?: boolean;
  nullable?: boolean | null;
};

export type SourceLocation = {
  path: string;
  line?: number | null;
  column?: number | null;
  endLine?: number | null;
  endColumn?: number | null;
};

export type SnapshotLink = {
  id: string;
  from: string;
  to: string;
  kind: "db_fk" | "code_call" | string;
  label?: string | null;
  truthClass?: "confirmed" | "structural" | "candidate" | "unknown" | string;
  direction?: string;
  engineEdgeType?: string | null;
  evidence?: { kind: string; text: string }[];
};

export type InventorySnapshot = {
  schemaVersion?: number;
  workspaceId: string;
  savedAt: string;
  metadata?: SnapshotMetadata;
  staleReasons?: string[];
  links?: SnapshotLink[];
  items: InventoryItem[];
};

export type SnapshotMetadata = {
  code?: SnapshotSourceMetadata | null;
  db?: SnapshotSourceMetadata | null;
  architecture?: unknown;
  migration?: SnapshotMigration;
  gaps?: SnapshotGap[];
};

export type SnapshotSourceMetadata = {
  savedAt: string;
  engineId?: string | null;
  engineVersion?: string | null;
  engineChecksum?: string | null;
  contractVersion?: string | null;
  snapshotKey?: string | null;
  limitRequested?: number | null;
  limitApplied?: number | null;
  limitClamped?: boolean | null;
  resultCount?: number | null;
  totalTables?: number | null;
  truncated?: boolean | null;
  sourcePath?: string | null;
  sourceType: string;
  profileId?: string | null;
};

export type SnapshotMigration = {
  sourceSchemaVersion?: number | null;
  reindexRequired?: boolean;
  notes?: string[];
};

export type SnapshotGap = {
  id: string;
  kind: string;
  message: string;
  relatedIds?: string[];
};
