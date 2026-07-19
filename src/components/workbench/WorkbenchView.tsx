import { LoaderCircle, ShieldCheck, X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { EngineRegistry } from "../../types/engine";
import { codeInventoryItemCount } from "../../types/workspace";
import { AtlasCanvas } from "../atlas/AtlasCanvas";
import { InspectorPanel } from "./InspectorPanel";
import { ModePanel } from "./ModePanel";
import { WorkbenchLeftPanel } from "./WorkbenchLeftPanel";
import { WorkbenchStatusBar } from "./WorkbenchStatusBar";
import { WorkbenchTopBar } from "./WorkbenchTopBar";

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
  const hasAnswerSource =
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasSnapshotState =
    hasAnswerSource ||
    visualMapControls.snapshotStaleReasons.length > 0 ||
    Boolean(visualMapControls.snapshotSavedAt);
  const showInspector = Boolean(
    hasAnswerSource &&
      (visualMapControls.selectedNode ||
        visualMapControls.selectedEdge ||
        (visualMapControls.mode !== "atlas" && (visualMapControls.currentMap || visualMapControls.loading))),
  );
  const drawerOpen = sourceManagerOpen && hasWorkspace;
  const sourceManagerRef = useRef<HTMLElement | null>(null);

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
    <div className="app-shell product-shell" data-view="workbench">
      <WorkbenchTopBar
        sourceManagerOpen={drawerOpen}
        onToggleSourceManager={() => setSourceManagerOpen(!drawerOpen)}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
      <div className={`workspace product-workspace ${showInspector ? "has-inspector" : ""}`}>
        <aside className="product-navigation" aria-label="주요 탐색">
          <ModePanel
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
            onNavigate={() => setSourceManagerOpen(false)}
            onOpenSources={() => setSourceManagerOpen(true)}
          />
        </aside>
        {!workspaceControls.initialized ? (
          <main className="workspace-initializing" aria-busy="true" aria-live="polite">
            <LoaderCircle className="spin" size={22} />
            <strong>프로젝트를 확인하고 있습니다</strong>
            <span>마지막으로 열었던 작업 공간을 준비합니다.</span>
          </main>
        ) : hasWorkspace ? (
          <AtlasCanvas
            openSourceManager={() => setSourceManagerOpen(true)}
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
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
            <InspectorPanel
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
            />
          </aside>
        )}
      </div>
      {hasSnapshotState && (
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
