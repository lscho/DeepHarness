# DeepHarness

DeepSeek Harness 桌面启动器。一个基于 Tauri 的跨平台桌面应用（macOS / Windows），负责在本地拉起 DeepSeek Harness（DSH）服务并打开其 Web 界面，免去手动在终端执行 `npx @deepseek-ai/dsh web` 的麻烦。

## 功能

- **一键启动本地服务**：应用启动时自动选择一个空闲端口，并通过登录 shell（自动加载 nvm）执行 `npx @deepseek-ai/dsh web` 启动 DSH 服务。
- **启动进度展示**：白色系启动界面实时展示服务状态（准备中 → 启动中 → 已就绪 / 出错），错误时给出提示。
- **自动跳转**：服务就绪后自动将窗口导航到本地 DSH Web 界面（`http://127.0.0.1:<port>/`），无需手动打开浏览器。
- **干净退出**：关闭应用时自动终止 DSH 服务进程（连同其子进程组），不留后台残留。

## 技术栈

| 部分     | 技术                               |
| -------- | ---------------------------------- |
| 桌面壳   | Tauri 2（Rust，跨平台：macOS / Windows） |
| 前端     | Vite + TypeScript + 原生 WebView   |
| 服务检测 | reqwest 轮询就绪检查（45s 超时）   |
| 测试     | Vitest（前端）+ Rust 单元测试      |

## 目录结构

```
├── src/                  # 前端：启动界面（状态渲染、样式）
│   ├── main.ts           # 监听 dsh-status 事件并渲染/跳转
│   ├── status.ts         # 启动状态渲染
│   └── style.css         # 白色系启动界面样式
├── src-tauri/            # Tauri 桌面壳
│   ├── src/lib.rs        # 启动/停止 DSH 服务、就绪探测、状态事件
│   └── tauri.conf.json   # 应用窗口与打包配置
├── tests/                # 前端单元测试
└── scripts/              # 打包辅助脚本
```

## 支持平台

- **macOS**：通过登录 shell（zsh + nvm）启动服务，退出时终止整个进程组；打包为 .app / .dmg（支持签名与公证）。
- **Windows**：通过 `cmd /C npx …` 启动服务（不弹控制台窗口），退出时用 `taskkill /T` 终止整个进程树；打包为 NSIS 安装包。

## 环境要求

- Node.js + npm
- Rust 工具链（构建 Tauri 需要）
- macOS 打包签名/公证需要 Apple Developer 证书与 App Store Connect API Key

## 开发

```bash
npm install

# 启动开发模式（热更新）
npm run tauri dev
```

## 构建

```bash
# 构建前端 + 打包桌面应用（.app / .dmg）
npm run tauri build
```

## 测试

```bash
# 前端测试（Vitest）
npm test

# Rust 单元测试
cd src-tauri && cargo test
```

## 说明

- DSH 服务通过 `npx @deepseek-ai/dsh web --port <port> --trusted-host 127.0.0.1:<port>` 启动，需要网络能访问 npm 仓库以获取包。
- 服务启动超时时间为 45 秒，超时后界面会显示错误信息。
- 当前界面语言为中文。
