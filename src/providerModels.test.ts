import { describe, expect, it } from "vitest";
import {
  DEFAULT_REASONING_EFFORT,
  defaultEffortFor,
  defaultModelFor,
  effortOptionsFor,
  modelsFor,
} from "./providerModels";

describe("CLI provider model contract", () => {
  it("offers the exact Codex model aliases used by the workbench", () => {
    expect(modelsFor("codex").map((model) => model.id)).toEqual([
      "gpt-5.6-sol",
      "gpt-5.6-terra",
      "gpt-5.6-luna",
      "gpt-5.5",
    ]);
    expect(defaultModelFor("codex", null)).toBe("gpt-5.6-sol");
  });

  it("uses high as the default reasoning effort", () => {
    expect(DEFAULT_REASONING_EFFORT).toBe("high");
    expect(defaultEffortFor("codex", "gpt-5.6-sol", null)).toBe("high");
    expect(defaultEffortFor("claude", "opus", null)).toBe("high");
  });

  it("does not expose an effort unsupported by a selected CLI model", () => {
    expect(effortOptionsFor("codex", "gpt-5.5").map((option) => option.id)).toEqual(["low", "medium", "high", "xhigh"]);
    expect(effortOptionsFor("claude", "opus").map((option) => option.id)).not.toContain("ultra");
  });
});
