export type DshStatus = {
  state: "starting" | "ready" | "error";
  message: string;
};

export function renderStatus(status: DshStatus): string {
  if (status.state === "error") {
    return `
      <main class="startup-card error-card">
        <p class="eyebrow">DEEP CODE</p>
        <h1>无法启动 DeepSeek Harness</h1>
        <p class="message">${status.message}</p>
        <p class="hint">请确认终端中可以运行 <code>npx @deepseek-ai/dsh web</code>。</p>
      </main>
    `;
  }

  return `
    <main class="startup-card">
      <p class="eyebrow">DEEP CODE</p>
      <h1>正在启动 DeepSeek Harness…</h1>
      <p class="message">${status.message}</p>
    </main>
  `;
}
