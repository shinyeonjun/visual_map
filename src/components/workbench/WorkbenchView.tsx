import { FileSearch, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { EngineRegistry } from "../../types/engine";
import { codeInventoryItemCount } from "../../types/workspace";
import { AtlasCanvas } from "../atlas/AtlasCanvas";
import { AnswerCanvas } from "./AnswerCanvas";
import { InspectorPanel } from "./InspectorPanel";
import { ModePanel } from "./ModePanel";
import { TargetNavigator } from "./TargetNavigator";
import { WorkbenchLeftPanel } from "./WorkbenchLeftPanel";
import { WorkbenchStatusBar } from "./WorkbenchStatusBar";
import { WorkbenchTopBar } from "./WorkbenchTopBar";
import { targetKindForMode } from "./targetModel";

export function WorkbenchView({
  sourceManagerOpen,
  setSourceManagerOpen,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
}: {
  sourceManagerOpen: boolean;
  setSourceManagerOpen: (open: boolean) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
}) {
  const [surface, setSurface] = useState<"answers" | "advanced">("answers");
  const [pendingSurface, setPendingSurface] = useState<"answers" | "advanced" | null>(null);
  const hasAnswerSource =
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasSnapshotState =
    hasAnswerSource ||
    visualMapControls.snapshotStaleReasons.length > 0 ||
    Boolean(visualMapControls.snapshotSavedAt);
  // Keep the evidence column mounted once a project exists so mode changes do not
  // resize the canvas. Its contents can change; the workspace geometry cannot.
  const showInspector = hasWorkspace;
  const inspectorVisible = Boolean(visualMapControls.selectedNode || visualMapControls.selectedEdge);
  const drawerOpen = sourceManagerOpen && hasWorkspace;
  const sourceManagerRef = useRef<HTMLElement | null>(null);
  const lastAnswerRef = useRef<{ workspaceId: string; mode: string; focusId: string } | null>(null);
  const workspaceId = workspaceControls.currentWorkspace?.id ?? null;
  const visibleMode = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.mode
    : visualMapControls.mode;
  const answerHasTarget = ["api-flow", "search-focus", "table-usage", "column-impact"].includes(visibleMode)
    && Boolean(answerFocusId(visualMapControls));

  useLayoutEffect(() => {
    lastAnswerRef.current = null;
    setPendingSurface(null);
    setSurface("answers");
  }, [workspaceId]);

  useLayoutEffect(() => {
    const committedMap = visualMapControls.currentMap;
    if (committedMap && targetKindForMode(committedMap.mode)) {
      const focusId = validAnswerFocus(committedMap.focus);
      if (workspaceId && focusId) {
        lastAnswerRef.current = { workspaceId, mode: committedMap.mode, focusId };
      }
    }

    if (visualMapControls.loading) return;
    const committedSurface = surfaceForMode(committedMap?.mode ?? visualMapControls.mode);
    if (pendingSurface) {
      if (committedSurface === pendingSurface) setSurface(pendingSurface);
      setPendingSurface(null);
    } else if (committedSurface === "answers") {
      setSurface("answers");
    }
  }, [
    pendingSurface,
    visualMapControls.loading,
    visualMapControls.mode,
    visualMapControls.currentMap?.mode,
    visualMapControls.currentMap?.focus,
    workspaceId,
  ]);

  function showAnswers() {
    if (surface === "advanced" && targetKindForMode(visualMapControls.mode)) {
      if (visualMapControls.loading) setPendingSurface("answers");
      else setSurface("answers");
      return;
    }
    const previous = lastAnswerRef.current;
    if (
      previous?.workspaceId === workspaceId &&
      !targetKindForMode(visualMapControls.mode)
    ) {
      setPendingSurface("answers");
      visualMapControls.showMode(previous.mode, previous.focusId);
      return;
    }
    setPendingSurface(null);
    setSurface("answers");
  }

  function showAdvanced(mode: "atlas" | "composition") {
    const focusId = answerFocusId(visualMapControls);
    if (workspaceId && focusId && targetKindForMode(visibleMode)) {
      lastAnswerRef.current = { workspaceId, mode: visibleMode, focusId };
    }
    setPendingSurface("advanced");
    visualMapControls.showMode(mode, null);
  }

  useEffect(() => {
    if (!hasWorkspace && sourceManagerOpen) {
      setSourceManagerOpen(false);
    }
  }, [hasWorkspace, sourceManagerOpen, setSourceManagerOpen]);

  useEffect(() => {
    if (!drawerOpen) {
      return;
    }
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    sourceManagerRef.current?.querySelector<HTMLElement>("button, input, select")?.focus();
    return () => previousFocus?.focus();
  }, [drawerOpen]);

  return (
    <div className="app-shell product-shell" data-view="workbench" data-surface={surface}>
      <WorkbenchTopBar
        key={workspaceId ?? "no-workspace"}
        sourceManagerOpen={drawerOpen}
        onToggleSourceManager={() => setSourceManagerOpen(!drawerOpen)}
        surface={pendingSurface ?? surface}
        onShowAnswers={showAnswers}
        onShowAdvanced={() => showAdvanced("atlas")}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
      <div className={`workspace product-workspace ${showInspector ? "has-inspector" : ""} ${inspectorVisible ? "inspector-visible" : ""}`}>
        <aside className="product-navigation" aria-label="주요 탐색">
          {surface === "advanced" ? (
            <ModePanel
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
              onNavigate={() => setSourceManagerOpen(false)}
              onOpenSources={() => setSourceManagerOpen(true)}
            />
          ) : (
            <TargetNavigator
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
              onSelectTarget={() => setSurface("answers")}
              onOpenAdvanced={showAdvanced}
            />
          )}
        </aside>
        {!workspaceControls.initialized ? (
          <main className="workspace-initializing" aria-busy="true" aria-live="polite">
            <LoaderCircle className="spin" size={22} />
            <strong>프로젝트를 확인하고 있습니다</strong>
            <span>마지막으로 열었던 작업 공간을 준비합니다.</span>
          </main>
        ) : hasWorkspace && surface === "advanced" ? (
          <AtlasCanvas
            openSourceManager={() => setSourceManagerOpen(true)}
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
          />
        ) : hasWorkspace ? (
          <AnswerCanvas
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
            onOpenSources={() => setSourceManagerOpen(true)}
          />
        ) : (
          <main className="source-onboarding-main">
            <header className="source-onboarding-heading">
              <span aria-hidden="true">1</span>
              <div>
                <h1>프로젝트를 연결하세요</h1>
                <p>코드 소스를 먼저 정하고, 데이터베이스는 필요할 때 이어서 연결합니다.</p>
              </div>
              <small>
                <ShieldCheck size={14} />
                분석은 이 기기에서 실행됩니다
              </small>
            </header>
            <WorkbenchLeftPanel
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
            />
          </main>
        )}
        {showInspector && (
          <aside className="side side-right evidence-panel">
            {surface === "advanced" || answerHasTarget ? (
              <InspectorPanel
                onClose={visualMapControls.clearSelection}
                title={surface === "answers" ? "근거" : "선택한 대상"}
                variant={surface === "answers" ? "answer" : "full"}
                workspaceControls={workspaceControls}
                dbProfileControls={dbProfileControls}
                visualMapControls={visualMapControls}
              />
            ) : (
              <section className="side-card inspector answer-evidence-placeholder">
                <div className="panel-header">
                  <FileSearch size={16} />
                  <h2>근거</h2>
                </div>
                <div>
                  <FileSearch size={21} />
                  <strong>대상을 선택하세요</strong>
                  <span>확정 근거, 확인할 후보, 파일 위치가 여기에 표시됩니다.</span>
                </div>
              </section>
            )}
          </aside>
        )}
      </div>
      {hasSnapshotState && surface === "advanced" && (
        <WorkbenchStatusBar
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
          engineRegistry={engineRegistry}
          engineError={engineError}
          devSlot={devSlot}
        />
      )}
      {drawerOpen && (
        <div
          className="source-manager-backdrop"
          role="presentation"
          onKeyDown={handleSourceManagerKeyDown}
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setSourceManagerOpen(false);
            }
          }}
        >
          <section ref={sourceManagerRef} className="source-manager" role="dialog" aria-modal="true" aria-label="소스 관리">
            <header className="source-manager-header">
              <span>
                <strong>소스 관리</strong>
                <small>코드와 데이터베이스 연결</small>
              </span>
              <button className="tool" type="button" onClick={() => setSourceManagerOpen(false)} aria-label="소스 관리 닫기">
                <X size={17} />
              </button>
            </header>
            <WorkbenchLeftPanel
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
            />
          </section>
        </div>
      )}
    </div>
  );

  function handleSourceManagerKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      setSourceManagerOpen(false);
      return;
    }
    if (event.key !== "Tab") {
      return;
    }
    const focusable = sourceManagerRef.current?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
    );
    if (!focusable?.length) {
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
}

function answerFocusId(visualMapControls: VisualMapControls): string | null {
  const value = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.focus
    : visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? null;
  return validAnswerFocus(value);
}

function validAnswerFocus(value: string | null | undefined): string | null {
  return value && value !== "narrow-focus" && value !== "overview" && !value.startsWith("group:") ? value : null;
}

function surfaceForMode(mode: string): "answers" | "advanced" | null {
  if (targetKindForMode(mode)) return "answers";
  return mode === "atlas" || mode === "composition" ? "advanced" : null;
}
