import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MapInspector } from "./MapInspector";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap, VisualNode } from "../../types/visual-map";

const node: VisualNode = {
  id: "grp_session_domain",
  kind: "group-domain",
  title: "세션 도메인",
  subtitle: "생성 흐름의 도메인 그룹|route|handler|table",
  layer: "mixed",
  source: "projection",
};

function controls(selectedNode: VisualNode | null): VisualMapControls {
  const map = {
    id: "map",
    workspaceId: "w",
    mode: "atlas",
    focus: "overview",
    nodes: selectedNode ? [selectedNode] : [],
    edges: [],
    warnings: [],
  } as unknown as VisualMap;
  return {
    currentMap: map,
    mode: "atlas",
    focusId: null,
    loading: false,
    selectedNode,
    selectedEdge: null,
    snapshotStaleReasons: [],
    clearSelection: vi.fn(),
    selectNode: vi.fn(),
    showMode: vi.fn(),
  } as unknown as VisualMapControls;
}

const workspace = {
  currentWorkspace: { id: "w", name: "shop" },
  codeInventory: null,
  repoSourceMode: "local",
  operationStatus: { phase: "idle", label: "", message: "" },
} as unknown as WorkspaceControls;

const dbControls = {
  inventory: null,
  activeProfile: null,
  profileName: "",
  profilePath: "",
  connectionString: "",
} as unknown as DbProfileControls;

describe("MapInspector identity", () => {
  it("names the selected target without opening a disclosure", () => {
    render(
      <MapInspector workspaceControls={workspace} dbProfileControls={dbControls} visualMapControls={controls(node)} />,
    );

    // The identity block sits above every section, so evidence is never shown
    // for a target the reader cannot name.
    const identity = screen.getByText("grp_session_domain").closest(".inspector-identity");
    expect(identity).not.toBeNull();
    expect(identity).toHaveTextContent("세션 도메인");
    // Only the human half of the pipe-delimited subtitle is surfaced.
    expect(identity).toHaveTextContent("생성 흐름의 도메인 그룹");
  });

  it("shows no identity block before anything is selected", () => {
    const { container } = render(
      <MapInspector workspaceControls={workspace} dbProfileControls={dbControls} visualMapControls={controls(null)} />,
    );

    expect(container.querySelector(".inspector-identity")).toBeNull();
  });
});
