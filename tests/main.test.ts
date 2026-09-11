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

  it("renders a ready card instead of navigating from the frontend", () => {
    const html = renderStatus({ state: "ready", message: "DeepSeek Harness 已就绪" });

    expect(html).toContain("DeepSeek Harness 已就绪");
    expect(html).toContain("正在打开本地界面");
  });

  it("escapes backend text instead of injecting markup", () => {
    const html = renderStatus({
      state: "error",
      message: 'http://127.0.0.1:43127/?token=a&b=<script>alert("x")</script>',
    });

    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("&amp;b=");
  });
});
