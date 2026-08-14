import { describe, expect, it } from "vitest";
import { renderStatus } from "../src/status";

describe("renderStatus", () => {
  it("explains that DeepSeek Harness is starting", () => {
    expect(renderStatus({ state: "starting", message: "正在启动" })).toContain(
      "正在启动 DeepSeek Harness",
    );
  });

  it("renders backend startup errors", () => {
    expect(renderStatus({ state: "error", message: "npx not found" })).toContain(
      "npx not found",
    );
  });
});
