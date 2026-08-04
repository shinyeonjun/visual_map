import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirm } from "@tauri-apps/plugin-dialog";
import { confirmAction } from "./confirmAction";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
}));

const mockedConfirm = vi.mocked(confirm);

describe("confirmAction", () => {
  beforeEach(() => {
    delete window.__TAURI_INTERNALS__;
    mockedConfirm.mockReset();
    vi.spyOn(window, "confirm").mockReturnValue(false);
  });

  it("uses the browser confirmation outside Tauri", async () => {
    vi.mocked(window.confirm).mockReturnValue(true);

    await expect(confirmAction("삭제할까요?")).resolves.toBe(true);
    expect(mockedConfirm).not.toHaveBeenCalled();
    expect(window.confirm).toHaveBeenCalledWith("삭제할까요?");
  });

  it("falls back without an unhandled rejection when the native command is unavailable", async () => {
    window.__TAURI_INTERNALS__ = {};
    mockedConfirm.mockRejectedValue(new Error("Command not found"));
    vi.mocked(window.confirm).mockReturnValue(true);

    await expect(confirmAction("삭제할까요?")).resolves.toBe(true);
    expect(mockedConfirm).toHaveBeenCalledOnce();
    expect(window.confirm).toHaveBeenCalledWith("삭제할까요?");
  });
});
