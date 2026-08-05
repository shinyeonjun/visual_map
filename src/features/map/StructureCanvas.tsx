import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Boxes,
  Braces,
  ChevronDown,
  ChevronRight,
  Cloud,
  Code2,
  Database,
  Hand,
  Home,
  LocateFixed,
  Map as MapIcon,
  Minus,
  MousePointer2,
  Plus,
} from "lucide-react";
import type { CSSProperties } from "react";
import { useFlowConnectors } from "../../hooks/useFlowConnectors";
import type { ConnectorRequest } from "../../hooks/useFlowConnectors";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { buildStructureTree, deriveModules, flattenStructure, isDerived, resolveConnectors } from "./structureModel";
import type { StructureNode } from "./structureModel";
import { useStructureViewport } from "./useStructureViewport";

const ROOT_PAGE_SIZE = 24;
const CHILD_PAGE_SIZE = 12;

/**
 * One canvas, four depths.
 *
 * Opening a box does not swap the screen and does not lift its neighbours into
 * a strip somewhere else: the box grows where it stands and its children appear
 * inside it, while every sibling keeps its place on the same canvas. That is
 * the whole point of the view — you can always see what the thing you opened
 * sits next to.
 *
 * Containment is the boxes; relations are the arrows drawn over them. An arrow
 * pointing into a closed box lands on the box itself rather than disappearing.
 */
export function StructureCanvas({
  areas,
  openId,
  revealPath,
  edges = [],
  map = null,
  revealedNode = null,
  loading = false,
  onExpandArea,
  onExpandNode,
  onCollapse,
  onSelectEdge,
  onSelectNode,
}: {
  areas: VisualNode[];
  openId?: string | null;
  revealPath?: string[];
  edges?: VisualEdge[];
  map?: VisualMap | null;
  revealedNode?: VisualNode | null;
  loading?: boolean;
  onExpandArea: (node: VisualNode) => void;
  onExpandNode?: (node: VisualNode) => void;
  onCollapse?: () => void;
  onSelectEdge?: (edge: VisualEdge) => void;
  onSelectNode?: (node: VisualNode) => void;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const [tool, setTool] = useState<"select" | "pan">("select");
  const [rootLimit, setRootLimit] = useState(ROOT_PAGE_SIZE);
  const viewport = useStructureViewport(map?.workspaceId ?? "empty-map");
  /*
    The expansion path is the canvas's own state, not the map's. The map holds
    one focus at a time, so driving containment from it collapsed every ancestor
    the moment the reader opened a child.
  */
  const [path, setPath] = useState<string[]>(() => (openId ? [openId] : []));
  /*
    The prop only wins when it names somewhere the canvas is not already
    standing. Deriving the path from it on every render meant "접기" reopened
    the box a frame later, because the map's focus had not changed yet.
  */
  const [syncedOpenId, setSyncedOpenId] = useState<string | null>(openId ?? null);
  const [syncedRevealPath, setSyncedRevealPath] = useState("");
  const revealSignature = revealPath?.join(">") ?? "";
  if (revealPath && revealSignature !== syncedRevealPath) {
    setSyncedRevealPath(revealSignature);
    setPath(revealPath);
  } else if ((openId ?? null) !== syncedOpenId) {
    setSyncedOpenId(openId ?? null);
    if (!openId) setPath([]);
    else if (!path.includes(openId)) setPath([openId]);
  }
  const activePath = path;

  /*
    Members arrive only for whichever node the map is focused on, so they are
    kept as they arrive. Without this, opening a child emptied its parent — the
    parent's members are not in the child's map.
  */
  const membersRef = useRef(new Map<string, VisualNode[]>());
  const ownerRef = useRef(new Map<string, string>());
  /*
    Keyed on what the reader asked to open, not on `map.focus`. The atlas map
    keeps `focus` at "overview" on some paths even while it carries the members
    of the requested group, and keying on focus meant the box opened onto
    nothing.
  */
  const pendingId = path[path.length - 1] ?? openId ?? null;
  if (map && pendingId) {
    const engineGroups = map.nodes.filter((node) => node.kind === "group-domain" && node.parentId === pendingId);
    const members = map.nodes.filter((node) => node.id !== pendingId && node.kind !== "group-domain");
    if (revealedNode && !members.some((node) => node.id === revealedNode.id)) members.push(revealedNode);
    const focusedGroup = map.nodes.find((node) => node.id === pendingId && node.kind === "group-domain");

    if (engineGroups.length > 0) {
      membersRef.current.set(pendingId, engineGroups);
      for (const group of engineGroups) ownerRef.current.set(group.id, pendingId);
    } else if (members.length > 0 && map.focus === pendingId) {
      if ((focusedGroup?.depth ?? 0) > 0) {
        membersRef.current.set(pendingId, members);
        for (const member of members) ownerRef.current.set(member.id, pendingId);
      } else {
        // Old snapshots do not carry module ownership. Keep path grouping only as
        // a compatibility fallback; current engine groups always win above.
        const { modules, membersOf } = deriveModules(pendingId, members);
        if (modules.length > 0) {
          membersRef.current.set(pendingId, modules);
          for (const module of modules) {
            ownerRef.current.set(module.id, pendingId);
            const owned = membersOf.get(module.id) ?? [];
            membersRef.current.set(module.id, owned);
            for (const member of owned) ownerRef.current.set(member.id, module.id);
          }
        } else {
          membersRef.current.set(pendingId, members);
          for (const member of members) ownerRef.current.set(member.id, pendingId);
        }
      }
    }
  }
  /*
    Not every surface has areas. An answer or a table-usage map carries its
    nodes directly, and keying the canvas only on `areas` left those surfaces
    rendering an empty board.
  */
  const roots = useMemo(
    () => (areas.length > 0 ? areas : (map?.nodes ?? []).filter((node) => node.kind !== "group-domain")),
    [areas, map],
  );
  const rootSignature = roots.map((node) => node.id).join("|");
  useEffect(() => setRootLimit(ROOT_PAGE_SIZE), [rootSignature]);
  const visibleRoots = useMemo(() => takeWithActive(roots, activePath[0], rootLimit), [activePath, rootLimit, roots]);
  for (const root of roots) ownerRef.current.delete(root.id);

  const childrenOf = useCallback((id: string) => membersRef.current.get(id) ?? [], []);
  /*
    The cache is filled during render, so the tree memo has to depend on its
    contents. Depending only on the props meant the members arrived one render
    too late and the opened box showed as empty.
  */
  const cacheSignature = [...membersRef.current.entries()]
    .map(([id, children]) => `${id}:${children.map((child) => child.id).join(",")}`)
    .join("|");
  const tree = useMemo(
    () => buildStructureTree(visibleRoots, childrenOf, activePath),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- cacheSignature stands in for the ref contents
    [visibleRoots, childrenOf, activePath, cacheSignature],
  );
  const visible = useMemo(() => flattenStructure(tree), [tree]);

  const allEdges = useMemo(() => [...edges, ...(map?.edges ?? [])], [edges, map]);
  const resolved = useMemo(
    () => resolveConnectors(allEdges, visible, (id) => ownerRef.current.get(id) ?? null),
    [allEdges, visible],
  );
  const requests = useMemo<ConnectorRequest[]>(
    () => resolved.map(({ id, from, to, tone, label }) => ({ id, from, to, tone, label })),
    [resolved],
  );
  const edgeById = useMemo(() => new Map(resolved.map((item) => [item.id, item])), [resolved]);

  const { connectors, width, height } = useFlowConnectors(
    canvasRef,
    requests,
    `${activePath.join(">")}|${visible.length}`,
  );

  const crumbs = useMemo(() => {
    const out: StructureNode[] = [];
    let level = tree;
    for (const id of activePath) {
      const found = level.find((node) => node.id === id);
      if (!found) break;
      out.push(found);
      level = found.children;
    }
    return out;
  }, [tree, activePath]);

  const open = (node: StructureNode) => {
    if (!node.expandable) {
      onSelectNode?.(node.node);
      return;
    }
    setPath([...activePath.slice(0, node.depth), node.id]);
    // A derived module exists only on this canvas. Its members are already
    // loaded under it, and handing its id to the map would clear the focus.
    if (isDerived(node.node)) return;
    if (node.depth === 0) onExpandArea(node.node);
    else if (onExpandNode) onExpandNode(node.node);
    onSelectNode?.(node.node);
  };

  const close = (node: StructureNode) => {
    const next = activePath.slice(0, node.depth);
    setPath(next);
    if (next.length === 0) onCollapse?.();
    else {
      const ancestor = visible.find((item) => item.id === next[next.length - 1]);
      if (ancestor) {
        if (ancestor.depth === 0) onExpandArea(ancestor.node);
        else onExpandNode?.(ancestor.node);
      }
    }
  };

  const toRoot = () => {
    setPath([]);
    onCollapse?.();
  };

  return (
    <div className="flow-canvas-shell" data-depth={activePath.length} data-tool={tool}>
      <header className="flow-toolbar">
        <div className="flow-toolbar-group" aria-label="캔버스 조작">
          <button
            type="button"
            className={tool === "select" ? "active" : ""}
            onClick={() => setTool("select")}
            aria-label="선택 도구"
            aria-pressed={tool === "select"}
          >
            <MousePointer2 size={15} />
          </button>
          <button
            type="button"
            className={tool === "pan" ? "active" : ""}
            onClick={() => setTool("pan")}
            aria-label="이동 도구"
            aria-pressed={tool === "pan"}
          >
            <Hand size={15} />
          </button>
          <button type="button" onClick={viewport.fitCanvas} aria-label="화면에 맞추기">
            <LocateFixed size={15} />
          </button>
        </div>

        <nav className="flow-breadcrumb" aria-label="구조 위치">
          <button
            type="button"
            onClick={toRoot}
            disabled={crumbs.length === 0 && map?.mode === "atlas" && map.focus === "overview"}
          >
            <Home size={13} aria-hidden="true" />
            전체 프로젝트
          </button>
          {crumbs.map((crumb) => (
            <span className="flow-crumb" key={crumb.id}>
              <ChevronRight size={12} aria-hidden="true" />
              <button type="button" onClick={() => open(crumb)}>
                {crumb.title}
              </button>
            </span>
          ))}
        </nav>

        <div className="flow-toolbar-group flow-zoom" aria-label="확대 축소">
          <button type="button" onClick={() => viewport.zoomBy(-0.1)} aria-label="축소">
            <Minus size={14} />
          </button>
          <button type="button" onClick={viewport.resetView} aria-label="확대율 초기화">
            {Math.round(viewport.zoom * 100)}%
          </button>
          <button type="button" onClick={() => viewport.zoomBy(0.1)} aria-label="확대">
            <Plus size={14} />
          </button>
        </div>

        <details className="flow-legend-popover">
          <summary aria-label="범례 열기">
            <MapIcon size={15} />
          </summary>
          <div className="flow-legend" aria-label="범례">
            <span>
              <i className="api" />
              API
            </span>
            <span>
              <i className="code" />
              코드
            </span>
            <span>
              <i className="db" />
              데이터
            </span>
            <span className="line confirmed">확정</span>
            <span className="line candidate">후보</span>
          </div>
        </details>
      </header>

      <div
        className="flow-stage"
        ref={viewport.stageRef}
        onPointerDown={(event) => viewport.startPan(event, tool === "pan")}
        onPointerMove={viewport.movePan}
        onPointerUp={viewport.stopPan}
        onPointerCancel={viewport.stopPan}
        onWheel={viewport.handleWheel}
        onScroll={viewport.remember}
      >
        <div className="flow-world" style={{ zoom: viewport.zoom } as CSSProperties}>
          <div
            className="flow-canvas"
            ref={canvasRef}
            role="region"
            aria-label={`${crumbs[crumbs.length - 1]?.title ?? "전체 프로젝트"} 구조 흐름`}
          >
            <svg
              className="flow-wires"
              width={width}
              height={height}
              viewBox={`0 0 ${Math.max(1, width)} ${Math.max(1, height)}`}
              aria-hidden="true"
            >
              <defs>
                {["confirmed", "candidate", "inferred"].map((tone) => (
                  <marker
                    id={`flow-arrow-${tone}`}
                    key={tone}
                    markerWidth="8"
                    markerHeight="8"
                    refX="7"
                    refY="4"
                    orient="auto"
                    markerUnits="userSpaceOnUse"
                  >
                    <path
                      d="M 0 0 L 8 4 L 0 8 z"
                      fill={
                        tone === "candidate" ? "var(--orange)" : tone === "inferred" ? "var(--ink-4)" : "var(--ink-3)"
                      }
                    />
                  </marker>
                ))}
              </defs>
              {connectors.map((connector) => (
                <path
                  className={`flow-wire ${connector.tone}`}
                  d={connector.path}
                  key={connector.id}
                  markerEnd={`url(#flow-arrow-${connector.tone})`}
                  onClick={
                    onSelectEdge
                      ? () => {
                          const found = edgeById.get(connector.id);
                          if (found) onSelectEdge(found.edge);
                        }
                      : undefined
                  }
                />
              ))}
            </svg>

            <div className="flow-level" data-level="0">
              {tree.map((node) => (
                <FlowBox
                  key={node.id}
                  node={node}
                  openPath={activePath}
                  loading={loading}
                  onOpen={open}
                  onClose={close}
                />
              ))}
              {roots.length > visibleRoots.length ? (
                <button
                  className="flow-more"
                  type="button"
                  onClick={() => setRootLimit((limit) => limit + ROOT_PAGE_SIZE)}
                >
                  <Plus size={15} aria-hidden="true" />
                  영역 {Math.min(ROOT_PAGE_SIZE, roots.length - visibleRoots.length).toLocaleString("ko-KR")}개 더 보기
                  <small>전체 {roots.length.toLocaleString("ko-KR")}개</small>
                </button>
              ) : null}
            </div>

            {tree.length === 0 ? <p className="flow-empty">아직 지도에 표시할 구조가 없습니다.</p> : null}
          </div>
        </div>
      </div>

      {tree.length > 0 ? (
        <button className="flow-minimap" type="button" onClick={viewport.fitCanvas} aria-label="미니맵에서 화면 맞춤">
          <span className="flow-minimap-map" aria-hidden="true">
            {roots.slice(0, 24).map((node) => (
              <i key={node.id} className={activePath[0] === node.id ? "active" : ""} />
            ))}
          </span>
          <small>{roots.length.toLocaleString("ko-KR")}개 영역</small>
        </button>
      ) : null}
    </div>
  );
}

function FlowBox({
  node,
  openPath,
  loading,
  onOpen,
  onClose,
}: {
  node: StructureNode;
  openPath: string[];
  loading: boolean;
  onOpen: (node: StructureNode) => void;
  onClose: (node: StructureNode) => void;
}) {
  const isOpen = openPath[node.depth] === node.id;
  const [childLimit, setChildLimit] = useState(CHILD_PAGE_SIZE);
  const visibleChildren = takeWithActive(node.children, openPath[node.depth + 1], childLimit);

  if (!isOpen) {
    return (
      <button
        className={`flow-box ${node.tone}`}
        data-flow-id={node.id}
        type="button"
        onClick={() => onOpen(node)}
        aria-expanded={node.expandable ? false : undefined}
        aria-label={`${node.title} ${node.kindLabel} ${node.expandable ? "펼치기" : "선택"}`}
      >
        <BoxFace node={node} />
      </button>
    );
  }

  return (
    <section className={`flow-box ${node.tone} is-open`} data-flow-id={node.id}>
      <header className="flow-box-head">
        <span className="flow-box-icon" aria-hidden="true">
          <BoxKindIcon tone={node.tone} />
        </span>
        <span className="flow-box-title">
          <strong>{node.title}</strong>
          <small>{node.meta}</small>
        </span>
        <span className="flow-box-depth">
          {node.kindLabel}
          {node.children.length > 0 ? ` · 하위 ${node.children.length.toLocaleString("ko-KR")}개` : ""}
        </span>
        <button
          className="flow-box-collapse"
          type="button"
          onClick={() => onClose(node)}
          aria-label={`${node.title} 접기`}
          aria-expanded
        >
          <ChevronDown size={14} aria-hidden="true" />
          접기
        </button>
      </header>
      {node.children.length > 0 ? (
        <div className="flow-level" data-level={node.depth + 1}>
          {visibleChildren.map((child) => (
            <FlowBox
              key={child.id}
              node={child}
              openPath={openPath}
              loading={loading}
              onOpen={onOpen}
              onClose={onClose}
            />
          ))}
          {node.children.length > visibleChildren.length ? (
            <button
              className="flow-more"
              type="button"
              onClick={() => setChildLimit((limit) => limit + CHILD_PAGE_SIZE)}
            >
              <Plus size={15} aria-hidden="true" />
              {Math.min(CHILD_PAGE_SIZE, node.children.length - visibleChildren.length).toLocaleString("ko-KR")}개 더
              보기
              <small>전체 {node.children.length.toLocaleString("ko-KR")}개</small>
            </button>
          ) : null}
        </div>
      ) : (
        <p className="flow-box-pending">
          {loading ? "하위 구조를 불러오는 중입니다." : "이 단계의 하위 구조는 없습니다."}
        </p>
      )}
    </section>
  );
}

function BoxFace({ node }: { node: StructureNode }) {
  return (
    <>
      <span className="flow-box-icon" aria-hidden="true">
        <BoxKindIcon tone={node.tone} />
      </span>
      <span className="flow-box-title">
        <span className="flow-box-kicker">{node.kindLabel}</span>
        <strong>{node.title}</strong>
        {node.meta ? <small>{node.meta}</small> : null}
      </span>
      <BoxMetrics node={node} />
    </>
  );
}

function BoxKindIcon({ tone }: { tone: StructureNode["tone"] }) {
  if (tone === "package") return <Boxes size={15} />;
  if (tone === "module") return <Box size={15} />;
  if (tone === "api") return <Braces size={15} />;
  if (tone === "db") return <Database size={15} />;
  if (tone === "external") return <Cloud size={15} />;
  return <Code2 size={15} />;
}

function BoxMetrics({ node }: { node: StructureNode }) {
  const metrics = node.node.metrics;
  if (!metrics && !node.metric) return null;
  return (
    <span className="flow-box-metric">
      {metrics ? (
        <>
          <span>API {metrics.apiCount.toLocaleString("ko-KR")}</span>
          <span>코드 {metrics.codeCount.toLocaleString("ko-KR")}</span>
          <span>DB {metrics.dbCount.toLocaleString("ko-KR")}</span>
          {(metrics.inDegree ?? 0) + (metrics.outDegree ?? 0) > 0 ? (
            <span className="relations">
              연결 {(metrics.inDegree ?? 0).toLocaleString("ko-KR")}← ·{" "}
              {(metrics.outDegree ?? 0).toLocaleString("ko-KR")}→
            </span>
          ) : null}
        </>
      ) : (
        node.metric
      )}
    </span>
  );
}

function takeWithActive<T extends { id: string }>(items: T[], activeId: string | undefined, limit: number): T[] {
  if (items.length <= limit) return items;
  const visible = items.slice(0, limit);
  if (!activeId || visible.some((item) => item.id === activeId)) return visible;
  const active = items.find((item) => item.id === activeId);
  return active ? [...visible.slice(0, Math.max(0, limit - 1)), active] : visible;
}
