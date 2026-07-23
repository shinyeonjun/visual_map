import {
  Braces,
  Columns3,
  Database,
  FileCode2,
  GitCompareArrows,
  Search,
} from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType, KeyboardEvent } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryCodeItems } from "../../types/workspace";
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

const DEFAULT_CODE_ITEMS_PER_GROUP = 12;

export function TargetNavigator({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onSelectTarget,
  onOpenDatabase,
  onOpenRelations,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onSelectTarget: () => void;
  onOpenDatabase: () => void;
  onOpenRelations: () => void;
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
  const kindChosenRef = useRef(false);
  const workspaceId = workspaceControls.currentWorkspace?.id ?? null;

  useEffect(() => {
    const nextKind = targetKindForMode(visibleMode);
    if (nextKind) setKind(nextKind);
  }, [visibleMode]);

  useEffect(() => {
    kindChosenRef.current = false;
    setQuery("");
    setKind(targetKindForMode(visualMapControls.mode) ?? firstAvailableTargetKind(catalog));
  }, [workspaceId]);

  useEffect(() => {
    if (kindChosenRef.current || catalog[kind].length > 0) return;
    const available = firstAvailableTargetKind(catalog);
    if (catalog[available].length > 0) setKind(available);
  }, [catalog, kind]);

  const normalizedQuery = query.trim().toLocaleLowerCase("ko-KR");
  const matchingItems = catalog[kind].filter((item) =>
    !normalizedQuery || [item.badge, item.title, item.meta, item.group]
      .some((value) => value?.toLocaleLowerCase("ko-KR").includes(normalizedQuery)),
  );
  const items = visibleTargetItems(matchingItems, kind, Boolean(normalizedQuery));
  const hiddenEngineCodeCount = kind === "code"
    ? codeInventoryCodeItems(workspaceControls.codeInventory).length
      + (workspaceControls.codeInventory?.files.length ?? 0)
      - catalog.code.length
    : 0;
  const fullSearchAvailable = kind === "code" && workspaceControls.codeInventory?.partial;
  const visibleCountLabel = matchingItems.length > items.length
    ? `${items.length.toLocaleString("ko-KR")}/${matchingItems.length.toLocaleString("ko-KR")}개 표시`
    : `${matchingItems.length.toLocaleString("ko-KR")}개`;
  const committedFocus = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.focus
    : visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? null;
  const activeItemIndex = items.findIndex(
    (item) => committedFocus === item.focusId && visibleMode === item.mode,
  );
  const targetTabStopIndex = activeItemIndex >= 0 ? activeItemIndex : 0;

  return (
    <section className="target-navigator" aria-label="분석 대상 탐색">
      <header className="target-navigator-header">
        <span>
          <strong>대상</strong>
          <small>하나를 선택하면 기본 답을 엽니다</small>
        </span>
      </header>

      <div className="target-kind-tabs" role="tablist" aria-label="대상 종류">
        {TARGET_KINDS.map(({ kind: targetKind, label, description, icon: Icon }) => (
          <button
            className={kind === targetKind ? "active" : ""}
            type="button"
            role="tab"
            id={`target-kind-${targetKind}`}
            data-target-kind={targetKind}
            aria-selected={kind === targetKind}
            aria-controls="target-list-panel"
            tabIndex={kind === targetKind ? 0 : -1}
            title={`${label}: ${description}`}
            onKeyDown={moveTargetKind}
            onClick={() => {
              kindChosenRef.current = true;
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

      <div
        className="target-list"
        id="target-list-panel"
        ref={listRef}
        role="tabpanel"
        aria-labelledby={`target-kind-${kind}`}
      >
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
                data-target-id={item.focusId}
                aria-current={active ? "true" : undefined}
                aria-busy={pending || undefined}
                tabIndex={index === targetTabStopIndex ? 0 : -1}
                title={`${item.title} · ${item.meta}`}
                onKeyDown={moveTargetItem}
                onClick={() => {
                  onSelectTarget();
                  if (!active) {
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
          <div className="target-list-empty">
            <p>
              {catalog[kind].length > 0
                ? "필터와 일치하는 대상이 없습니다."
                : kind === "table" || kind === "column"
                  ? dbProfileControls.inventory
                    ? `읽은 DB에서 ${kind === "table" ? "테이블" : "컬럼"} 정보를 찾지 못했습니다.`
                    : `DB를 연결하면 ${kind === "table" ? "테이블 사용 위치" : "컬럼 변경 영향"}를 볼 수 있습니다.`
                  : `${TARGET_KINDS.find((item) => item.kind === kind)?.label ?? "대상"} 정보를 아직 읽지 못했습니다.`}
            </p>
            {catalog[kind].length === 0 && (kind === "table" || kind === "column") ? (
              <button type="button" onClick={onOpenDatabase}>
                <Database size={14} />
                <span>{dbProfileControls.inventory ? "DB 소스 확인" : "DB 연결"}</span>
              </button>
            ) : null}
          </div>
        ) : null}
      </div>

      <footer className="target-navigator-footer">
        <small>
          <span>
            {visibleCountLabel}
            {fullSearchAvailable ? " · 전체는 상단 검색" : ""}
          </span>
          {hiddenEngineCodeCount > 0
            ? <span>내장 심볼 {hiddenEngineCodeCount.toLocaleString("ko-KR")}개 제외</span>
            : ""}
        </small>
        <button type="button" title="여러 대상을 선택해 관계 보기" onClick={onOpenRelations}>
          <GitCompareArrows size={15} />
          <span>여러 대상 관계</span>
        </button>
      </footer>
    </section>
  );
}

function moveTargetKind(event: KeyboardEvent<HTMLButtonElement>) {
  const tabs = Array.from(
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? [],
  );
  const currentIndex = tabs.indexOf(event.currentTarget);
  const nextIndex = event.key === "Home"
    ? 0
    : event.key === "End"
      ? tabs.length - 1
      : event.key === "ArrowRight" || event.key === "ArrowDown"
        ? (currentIndex + 1) % tabs.length
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? (currentIndex - 1 + tabs.length) % tabs.length
          : -1;
  if (currentIndex < 0 || nextIndex < 0) return;
  event.preventDefault();
  tabs[nextIndex]?.focus();
  tabs[nextIndex]?.click();
}

function moveTargetItem(event: KeyboardEvent<HTMLButtonElement>) {
  const items = Array.from(
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("button[data-target-id]") ?? [],
  );
  const currentIndex = items.indexOf(event.currentTarget);
  const nextIndex = event.key === "Home"
    ? 0
    : event.key === "End"
      ? items.length - 1
      : event.key === "ArrowDown"
        ? Math.min(currentIndex + 1, items.length - 1)
        : event.key === "ArrowUp"
          ? Math.max(currentIndex - 1, 0)
          : -1;
  if (currentIndex < 0 || nextIndex < 0 || nextIndex === currentIndex) return;
  event.preventDefault();
  items.forEach((item, index) => {
    item.tabIndex = index === nextIndex ? 0 : -1;
  });
  items[nextIndex]?.focus();
}

function visibleTargetItems(items: ReturnType<typeof buildTargetCatalog>[TargetKind], kind: TargetKind, hasQuery: boolean) {
  if (kind !== "code" || hasQuery) return items.slice(0, 100);
  const counts = new Map<string, number>();
  return items.filter((item) => {
    const group = item.group ?? "기타";
    const count = counts.get(group) ?? 0;
    if (count >= DEFAULT_CODE_ITEMS_PER_GROUP) return false;
    counts.set(group, count + 1);
    return true;
  }).slice(0, 100);
}
