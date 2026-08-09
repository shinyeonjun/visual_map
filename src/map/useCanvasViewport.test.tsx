import { render } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it } from "vitest";
import { useCanvasViewport } from "./useCanvasViewport";

/**
 * The canvas has to claim the wheel itself. A trackpad pinch reaches the page
 * as ctrl+wheel, which the WebView otherwise spends on zooming the whole
 * application, and React's own onWheel prop is passive so it cannot refuse
 * that. These tests exercise the real listener on a real element.
 */

type Viewport = ReturnType<typeof useCanvasViewport>;

function mountCanvas(): { element: HTMLElement; read: () => Viewport } {
  let latest: Viewport | null = null;
  function Harness() {
    const canvas = useCanvasViewport(1000, 800);
    latest = canvas;
    return <div data-testid="canvas" ref={canvas.viewRef} {...canvas.handlers} />;
  }
  // Scope the lookup to this render: several tests mount two canvases at once
  // to compare gestures, and a document-wide query would find both.
  const { container } = render(<Harness />);
  const element = container.querySelector<HTMLElement>("[data-testid='canvas']");
  if (!element) throw new Error("viewport canvas did not render");
  return {
    element,
    read: () => {
      if (!latest) throw new Error("viewport hook did not render");
      return latest;
    },
  };
}

function wheel(element: HTMLElement, init: WheelEventInit): WheelEvent {
  const event = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  act(() => {
    element.dispatchEvent(event);
  });
  return event;
}

describe("useCanvasViewport wheel handling", () => {
  it("takes the wheel away from the WebView so ctrl+wheel never becomes page zoom", () => {
    const { element } = mountCanvas();
    expect(wheel(element, { deltaY: -40, ctrlKey: true }).defaultPrevented).toBe(true);
    expect(wheel(element, { deltaY: 40 }).defaultPrevented).toBe(true);
  });

  it("pinching out zooms in and pinching in zooms out", () => {
    const { element, read } = mountCanvas();
    const start = read().view.scale;

    wheel(element, { deltaY: -20, ctrlKey: true });
    const zoomedIn = read().view.scale;
    expect(zoomedIn).toBeGreaterThan(start);

    wheel(element, { deltaY: 20, ctrlKey: true });
    expect(read().view.scale).toBeLessThan(zoomedIn);
  });

  it("handles a pinch that starts over a nested area instead of only bare canvas ground", () => {
    const { element, read } = mountCanvas();
    const area = document.createElement("div");
    element.appendChild(area);

    wheel(area, { deltaY: -20, ctrlKey: true });

    expect(read().view.scale).toBeGreaterThan(1);
  });

  it("scales zoom with the gesture instead of stepping a fixed amount", () => {
    const gentle = mountCanvas();
    wheel(gentle.element, { deltaY: -4, ctrlKey: true });

    const firm = mountCanvas();
    wheel(firm.element, { deltaY: -24, ctrlKey: true });

    // A fixed per-event step made a trackpad pinch, which fires dozens of
    // small deltas, slam straight into the zoom limit.
    expect(gentle.read().view.scale).toBeGreaterThan(1);
    expect(firm.read().view.scale).toBeGreaterThan(gentle.read().view.scale);
  });

  it("caps one mouse-wheel notch so it cannot jump several zoom levels", () => {
    const { element, read } = mountCanvas();
    wheel(element, { deltaY: -1000, ctrlKey: true });
    expect(read().view.scale).toBeCloseTo(1.25, 5);
  });

  it("reads line and page deltas as pixels so panning matches the device", () => {
    const pixels = mountCanvas();
    wheel(pixels.element, { deltaY: 16, deltaMode: 0 });

    const lines = mountCanvas();
    wheel(lines.element, { deltaY: 1, deltaMode: 1 });

    // One line is not one pixel; without this the map crawled on a mouse.
    expect(lines.read().view.y).toBe(pixels.read().view.y);
    expect(lines.read().view.y).toBe(-16);
  });

  it("pans without ctrl and never zooms on a plain two-finger scroll", () => {
    const { element, read } = mountCanvas();
    wheel(element, { deltaX: 30, deltaY: -50 });
    expect(read().view).toMatchObject({ x: -30, y: 50, scale: 1 });
  });
});
