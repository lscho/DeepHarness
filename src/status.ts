export type DshStatus = {
  state: "starting" | "ready" | "error";
  message: string;
};

/**
 * Escape text that is interpolated into the status card.
 *
 * Messages carry backend output (spawn errors, the resolved DSH URL), so they
 * are treated as text, never as markup.
 */
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function renderStatus(status: DshStatus): string {
  const message = escapeHtml(status.message);

  if (status.state === "error") {
    return `
      <main class="startup-card error-card">
        <p class="eyebrow">DeepSeek Harness</p>
        <h1>无法启动 DeepSeek Harness</h1>
        <p class="message">${message}</p>
        <p class="hint">请确认终端中可以运行 <code>npx @deepseek-ai/dsh web</code>。</p>
      </main>
    `;
  }

  if (status.state === "ready") {
    // The launcher window itself navigates to the authenticated DSH URL, so
    // this card is only visible for the moment before that navigation lands.
    return `
      <main class="startup-card">
        <p class="eyebrow">DeepSeek Harness</p>
        <h1>DeepSeek Harness 已就绪</h1>
        <p class="message">正在打开本地界面…</p>
      </main>
    `;
  }

  return `
    <main class="startup-card">
      <p class="eyebrow">DeepSeek Harness</p>
      <h1>正在启动 DeepSeek Harness…</h1>
      <p class="message">${message}</p>
    </main>
  `;
}
