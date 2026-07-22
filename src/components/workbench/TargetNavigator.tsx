import {
  Braces,
  Columns3,
  Database,
  FileCode2,
  GitCompareArrows,
  Network,
  Search,
} from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import {
  buildTargetCatalog,
  firstAvailableTargetKind,
  targetKindForMode,
  type TargetKind,
} from "./targetModel";

type TargetIcon = ComponentType<{ size?: number }>;

const TARGET_KINDS: Array<{
  kind: TargetKind;
  label: string;
  description: string;
  icon: TargetIcon;
}> = [
  { kind: "api", label: "API", description: "처리 흐름", icon: Braces },
  { kind: "code", label: "코드", description: "호출 경로", icon: FileCode2 },
  { kind: "table", label: "테이블", description: "사용 위치", icon: Database },
  { kind: "column", label: "컬럼", description: "변경 영향", icon: Columns3 },
];

export function TargetNavigator({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onSelectTarget,
  onOpenAdvanced,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onSelectTarget: () => void;
  onOpenAdvanced: (mode: "atlas" | "composition") => void;
}) {
  const catalog = useMemo(
    () => buildTargetCatalog(workspaceControls.codeInventory, dbProfileControls.inventory),
    [workspaceControls.codeInventory, dbProfileControls.inventory],
  );
  const visibleMode = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.mode
    : visualMapControls.mode;
  const [kind, setKind] = useState<TargetKind>(() => targetKindForMode(visibleMode) ?? firstAvailableTargetKind(catalog));
  const [query, setQuery] = useState("");
  const listRef = useRef<HTMLDivElement | null>(null);
  const workspaceId = workspaceControls.currentWorkspace?.id ?? null;

  useEffect(() => {
    const nextKind = targetKindForMode(visibleMode);
    if (nextKind) setKind(nextKind);
  }, [visibleMode]);

  useEffect(() => {
    setQuery("");
    setKind(targetKindForMode(visualMapControls.mode) ?? firstAvailableTargetKind(catalog));
  }, [workspaceId]);

  useEffect(() => {
    if (catalog[kind].length === 0) {
      const available = firstAvailableTargetKind(catalog);
      if (catalog[available].length > 0) setKind(available);
    }
  }, [catalog, kind]);

  const normalizedQuery = query.trim().toLocaleLowerCase("ko-KR");
  const matchingItems = catalog[kind].filter((item) =>
    !normalizedQuery || [item.badge, item.title, item.meta, item.group]
      .some((value) => value?.toLocaleLowerCase("ko-KR").includes(normalizedQuery)),
  );
  const items = matchingItems.slice(0, 100);
  const committedFocus = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.focus
    : visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? null;

  return (
    <section className="target-navigator" aria-label="분석 대상 탐색">
      <header className="target-navigator-header">
        <span>
          <strong>대상 찾기</strong>
          <small>선택하면 알맞은 답을 바로 엽니다</small>
        </span>
      </header>

      <div className="target-kind-tabs" role="tablist" aria-label="대상 종류">
        {TARGET_KINDS.map(({ kind: targetKind, label, description, icon: Icon }) => (
          <button
            className={kind === targetKind ? "active" : ""}
            type="button"
            role="tab"
            aria-selected={kind === targetKind}
            title={`${label}: ${description}`}
            onClick={() => {
              setKind(targetKind);
              setQuery("");
              listRef.current?.scrollTo({ top: 0 });
            }}
            key={targetKind}
          >
            <Icon size={16} />
            <span>{label}</span>
            <em>{catalog[targetKind].length.toLocaleString("ko-KR")}</em>
          </button>
        ))}
      </div>

      <label className="target-filter">
        <Search size={14} aria-hidden="true" />
        <input
          value={query}
          disabled={catalog[kind].length === 0}
          aria-label={`${TARGET_KINDS.find((item) => item.kind === kind)?.label ?? "대상"} 목록 필터`}
          placeholder="현재 목록에서 찾기"
          onChange={(event) => {
            setQuery(event.currentTarget.value);
            listRef.current?.scrollTo({ top: 0 });
          }}
        />
      </label>

      <div className="target-list" ref={listRef} role="tabpanel">
        {items.map((item, index) => {
          const active = committedFocus === item.focusId && visibleMode === item.mode;
          const pending = visualMapControls.loading && visualMapControls.focusId === item.focusId && !active;
          const showGroup = item.group && item.group !== items[index - 1]?.group;
          return (
            <Fragment key={item.id}>
              {showGroup ? <h3>{item.group}</h3> : null}
              <button
                className={`${active ? "active" : ""}${pending ? " pending" : ""}`}
                type="button"
                aria-current={active ? "true" : undefined}
                aria-busy={pending || undefined}
                title={`${item.title} · ${item.meta}`}
                onClick={() => {
                  onSelectTarget();
                  const focusedNode = active
                    ? visualMapControls.currentMap?.nodes.find((node) => node.id === item.focusId) ?? null
                    : null;
                  if (focusedNode) {
                    visualMapControls.selectNode(focusedNode);
                  } else {
                    visualMapControls.showMode(item.mode, item.focusId);
                  }
                }}
              >
                <span>{item.badge}</span>
                <strong>{item.title}</strong>
                <small>{item.meta}</small>
                {active ? <em>현재</em> : null}
              </button>
            </Fragment>
          );
        })}
        {items.length === 0 ? (
          <p className="target-list-empty">
            {catalog[kind].length === 0
              ? `${TARGET_KINDS.find((item) => item.kind === kind)?.label ?? "대상"} 정보를 아직 읽지 못했습니다.`
              : "필터와 일치하는 대상이 없습니다."}
          </p>
        ) : null}
      </div>

      <footer className="target-navigator-footer">
        {matchingItems.length > items.length ? (
          <small>{items.length}개 표시 · 검색으로 범위를 좁히세요</small>
        ) : (
          <small>{matchingItems.length.toLocaleString("ko-KR")}개</small>
        )}
        <div>
          <button type="button" title="프로젝트 전체 구조 열기" onClick={() => onOpenAdvanced("atlas")}>
            <Network size={15} />
            <span>전체 구조</span>
          </button>
          <button type="button" title="선택한 여러 대상의 관계 보기" onClick={() => onOpenAdvanced("composition")}>
            <GitCompareArrows size={15} />
            <span>선택 관계</span>
          </button>
        </div>
      </footer>
    </section>
  );
}
