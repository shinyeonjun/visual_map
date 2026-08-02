import { Box, Braces, Database, FileCode2, GitBranch, Table2 } from "lucide-react";
import type { ComponentType } from "react";
import type { VisualEdge, VisualNode } from "../../types/visual-map";

type RelationLaneMeta = {
  label: string;
  tone: string;
  icon: ComponentType<{ size?: number }>;
};

/**
 * Shared role metadata for answer-canvas diagrams (code call graph, table ERD).
 * Mirrors the lane language of the API connection view so all four tabs read
 * as one product: same tones, same icons, same relation vocabulary.
 */
export function relationLaneMeta(lane: string): RelationLaneMeta {
  if (lane === "route") return { label: "Backend API", tone: "route", icon: Braces };
  if (lane === "client-request") return { label: "Client Request", tone: "client-request", icon: GitBranch };
  if (lane === "handler") return { label: "Handler", tone: "handler", icon: Box };
  if (lane === "repository-query") return { label: "Repository / Query", tone: "repository", icon: Database };
  if (lane === "database") return { label: "DB Table", tone: "database", icon: Table2 };
  return { label: "Service / Function", tone: "service", icon: FileCode2 };
}

export function relationNodeLane(node: VisualNode): string {
  const kind = node.kind.toLowerCase();
  if (node.layer === "db" || node.layer === "database" || kind === "table" || kind === "column") return "database";
  if (node.layer === "api" || kind === "api" || kind === "route") return "route";
  if (kind.includes("handler")) return "handler";
  if (kind.includes("repository")) return "repository-query";
  return "service-function";
}

export function relationEdgeShortLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  if (edge.kind === "client_request") return "REQUESTS";
  if (edge.kind === "code_db_read") return "READS";
  if (edge.kind === "code_db_write") return "WRITES";
  if (edge.kind === "code_db_uses_column") return "USES";
  if (edge.kind === "contains" || edge.kind === "group_contains") return "포함";
  if (edge.kind === "code_flow") return "이름 단서";
  if (isCandidateRelationEdge(edge)) return "후보";
  return edge.kind;
}

export function isCandidateRelationEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate") || edge.confidence === "candidate";
}

export function relationSourceLabel(location?: { path: string; line?: number | null } | null): string | null {
  if (!location) return null;
  return `${location.path}${location.line ? `:${location.line}` : ""}`;
}
