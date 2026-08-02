import { LoaderCircle, Maximize2, Minus, Plus, X } from "lucide-react";
import type { CodeInventory } from "../../types/workspace";
import type { VisualMapControls } from "../../types/controls";

export type FocusStripState = {
  label: string;
  title: string;
  meta: string;
  hint: string;
  tone: "code" | "db" | "edge" | "neutral";
};

type TransitionDescriptor = {
  title: string;
  purpose: string;
  lanes: string[];
  detailLanes: number;
};

export function CanvasTransitionState({
  descriptor,
  focus,
  mode,
}: {
  descriptor: TransitionDescriptor;
  focus: FocusStripState;
  mode: string;
}) {
  const apiMode = mode === "api-flow";

  return (
    <main className="canvas at-canvas is-transitioning" aria-busy="true">
      <div className={`at-canvas-head${apiMode ? " api-reading-head" : ""}`}>
        <div className="at-title-block">
          <strong>{descriptor.title}</strong>
          <span>{descriptor.purpose}</span>
        </div>
        <div className="at-transition-progress" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={13} />
          새 근거 구성 중
        </div>
        {!apiMode ? (
          <div className="at-canvas-controls" aria-hidden="true">
            <button className="tool" type="button" disabled><Maximize2 size={14} /></button>
            <button className="tool wide" type="button" disabled>100%</button>
            <button className="tool" type="button" disabled><Plus size={14} /></button>
            <button className="tool" type="button" disabled><Minus size={14} /></button>
          </div>
        ) : null}
      </div>
      {!apiMode ? <FocusStrip focus={focus} onClear={null} /> : null}
      <div className="at-stage">
        <div className={`at-transition-map mode-${mode}`} aria-label={`${descriptor.title} 로딩 상태`}>
          {descriptor.lanes.map((lane, index) => (
            <section className="at-transition-lane" key={lane}>
              <header>
                <span>{String(index + 1).padStart(2, "0")}</span>
                <strong>{lane}</strong>
              </header>
              <div className="at-transition-card" aria-hidden="true">
                <i />
                <b />
                <small />
              </div>
              {index < descriptor.detailLanes ? (
                <div className="at-transition-card compact" aria-hidden="true">
                  <i />
                  <b />
                  <small />
                </div>
              ) : null}
            </section>
          ))}
        </div>
      </div>
    </main>
  );
}

export function CompositionToolbar({
  visualMapControls,
  codeInventory,
  selectionLabel,
}: {
  visualMapControls: VisualMapControls;
  codeInventory: CodeInventory | null;
  selectionLabel: (nodeId: string, codeInventory: CodeInventory | null, map: VisualMapControls["currentMap"]) => string;
}) {
  const views = [
    ["connections", "전체 연결"],
    ["calls", "호출"],
    ["data", "데이터"],
    ["impact", "영향"],
  ] as const;

  return (
    <section className="composition-toolbar" aria-label="관계 분석 범위">
      <header>
        <strong>대상</strong>
        <span>{visualMapControls.compositionFocusIds.length}/8</span>
      </header>
      <div className="composition-targets">
        {visualMapControls.compositionFocusIds.length > 0 ? (
          visualMapControls.compositionFocusIds.map((id) => {
            const label = selectionLabel(id, codeInventory, visualMapControls.currentMap);
            return (
              <button
                type="button"
                title={`${label} 선택 해제`}
                aria-label={`${label} 선택 해제`}
                onClick={() => visualMapControls.toggleCompositionFocus(id)}
                key={id}
              >
                <span>{label}</span>
                <X size={12} />
              </button>
            );
          })
        ) : (
          <span className="composition-target-placeholder">선택 대기</span>
        )}
      </div>
      <div className="composition-view-switch" role="group" aria-label="관계 보기 방식">
        {views.map(([id, label]) => (
          <button
            className={visualMapControls.relationView === id ? "active" : ""}
            type="button"
            aria-pressed={visualMapControls.relationView === id}
            onClick={() => visualMapControls.setRelationView(id)}
            key={id}
          >
            {label}
          </button>
        ))}
      </div>
      <button
        className="composition-clear"
        type="button"
        title="분석 대상 전체 해제"
        aria-label="분석 대상 전체 해제"
        disabled={visualMapControls.compositionFocusIds.length === 0}
        onClick={visualMapControls.clearCompositionFocus}
      >
        <X size={14} />
      </button>
    </section>
  );
}

export function FocusStrip({ focus, onClear }: { focus: FocusStripState; onClear: (() => void) | null }) {
  return (
    <div className={`at-focus-strip ${focus.tone}`}>
      <span>{focus.label}</span>
      <strong title={focus.title}>{focus.title}</strong>
      <em>{focus.meta}</em>
      <small>
        <b>다음 행동</b>
        <i>{focus.hint}</i>
      </small>
      {onClear && (
        <button className="at-focus-clear" type="button" title="선택 해제" aria-label="선택 해제" onClick={onClear}>
          <X size={13} />
        </button>
      )}
    </div>
  );
}
