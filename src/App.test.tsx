import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("target product shell", () => {
  it("opens on one honest canvas and keeps chat disabled without a project", async () => {
    render(<App />);
    const heading = await screen.findByRole("heading", { level: 1 });
    expect(heading).toHaveTextContent(/코드를 읽기 전에\s*구조부터 봅니다/);
    expect(screen.getByRole("main", { name: "코드베이스 구조 지도" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "지도에 질문" })).toBeDisabled();
    expect(screen.queryByText("api-flow")).not.toBeInTheDocument();
  });
});
