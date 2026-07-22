import { beforeEach, describe, expect, it } from "vitest";
import { resetMapContext, saveMapContext, savedMapContext, savedModeMapContext } from "./mapContext";

describe("mapContext", () => {
  beforeEach(() => localStorage.clear());

  it("restores the last focus independently for each mode", () => {
    saveMapContext("workspace-1", "atlas", "group:accounts");
    saveMapContext("workspace-1", "api-flow", "code:route-1");
    saveMapContext("workspace-1", "table-usage", "db:table:public.users");

    expect(savedMapContext("workspace-1")).toEqual({
      mode: "table-usage",
      focusId: "db:table:public.users",
    });
    expect(savedModeMapContext("workspace-1", "atlas")).toEqual({
      mode: "atlas",
      focusId: "group:accounts",
    });
    expect(savedModeMapContext("workspace-1", "api-flow")).toEqual({
      mode: "api-flow",
      focusId: "code:route-1",
    });
    expect(savedModeMapContext("workspace-1", "column-impact")).toBeNull();
  });

  it("drops every stale per-mode focus when a data source is reset", () => {
    saveMapContext("workspace-1", "table-usage", "db:table:public.users");
    saveMapContext("workspace-1", "column-impact", "db:column:public.users:id");

    resetMapContext("workspace-1");

    expect(savedMapContext("workspace-1")).toEqual({ mode: "atlas", focusId: null });
    expect(savedModeMapContext("workspace-1", "table-usage")).toBeNull();
    expect(savedModeMapContext("workspace-1", "column-impact")).toBeNull();
  });
});
