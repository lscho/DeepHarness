import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";
import { renderStatus, type DshStatus } from "./status";

export { renderStatus } from "./status";

const app = document.querySelector<HTMLDivElement>("#app");

function applyStatus(status: DshStatus) {
  // Navigation to the DSH Web GUI is owned by the Rust side: only it knows the
  // per-process authenticated URL (`http://127.0.0.1:<port>/?token=…`) that DSH
  // prints at startup, and the bare origin answers 401 without it.
  if (app) {
    app.innerHTML = renderStatus(status);
  }
}

applyStatus({ state: "starting", message: "正在准备本地服务" });

void (async () => {
  await listen<DshStatus>("dsh-status", (event) => applyStatus(event.payload));
  applyStatus(await invoke<DshStatus>("dsh_status"));
})();
