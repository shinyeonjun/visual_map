import { OverviewCanvas } from "./OverviewCanvas";
import { TraceCanvas } from "./TraceCanvas";
import type { MapTrace, MapView } from "./types";

/**
 * The two questions the map answers, and which one is on screen.
 *
 * "What is this made of" is a field of areas and the relations between them.
 * "What happens when this runs" is an ordered flow. They are different shapes
 * because they are different questions, and the reader moves between them
 * deliberately — selecting an area describes it, opening its flow leaves for
 * it — rather than by zooming into an ambiguous middle.
 */

export interface TraceView {
  areaId: string;
  title: string;
  summary: string;
  traces: MapTrace[];
}

interface MapCanvasProps {
  view: MapView;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** Present while the reader is inside one area's execution paths. */
  traceView: TraceView | null;
  onOpenTrace: (areaId: string) => void;
  onCloseTrace: () => void;
}

export function MapCanvas({ view, selectedId, onSelect, traceView, onOpenTrace, onCloseTrace }: MapCanvasProps) {
  if (traceView) {
    return (
      <TraceCanvas
        title={traceView.title}
        summary={traceView.summary}
        traces={traceView.traces}
        selectedId={selectedId}
        onSelect={onSelect}
        onBack={onCloseTrace}
      />
    );
  }
  return <OverviewCanvas view={view} selectedId={selectedId} onSelect={onSelect} onOpenTrace={onOpenTrace} />;
}
