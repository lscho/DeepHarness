# DeepHarness

DeepSeek Harness 桌面启动器。一个基于 Tauri 的跨平台桌面应用（macOS / Windows），负责在本地拉起 DeepSeek Harness（DSH）服务并打开其 Web 界面，免去手动在终端执行 `npx @deepseek-ai/dsh web` 的麻烦。

## 功能

- **一键启动本地服务**：应用启动时自动选择一个空闲端口，并通过登录 shell（自动加载 nvm）执行 `npx @deepseek-ai/dsh web` 启动 DSH 服务。
- **启动进度展示**：白色系启动界面实时展示服务状态（准备中 → 启动中 → 已就绪 / 出错），错误时给出提示。
- **自动跳转**：服务就绪后自动将窗口导航到本地 DSH Web 界面（`http://127.0.0.1:<port>/`），无需手动打开浏览器。
- **干净退出**：关闭应用时自动终止 DSH 服务进程（连同其子进程组），不留后台残留。

## 环境要求

- Node.js + npm

## 说明

- DSH 服务通过 `npx @deepseek-ai/dsh web --port <port> --trusted-host 127.0.0.1:<port>` 启动，需要网络能访问 npm 仓库以获取包。
- 服务启动超时时间为 45 秒，超时后界面会显示错误信息。
