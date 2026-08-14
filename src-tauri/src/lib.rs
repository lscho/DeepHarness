use std::{
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager, RunEvent};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(45);

pub struct AppState {
    child: Mutex<Option<Child>>,
    status: Mutex<DshStatus>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            status: Mutex::new(DshStatus {
                state: "starting",
                message: "正在准备本地服务".to_owned(),
            }),
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DshStatus {
    state: &'static str,
    message: String,
}

fn current_dsh_status(state: &AppState) -> DshStatus {
    state.status.lock().expect("status lock poisoned").clone()
}

#[tauri::command]
fn dsh_status(state: tauri::State<AppState>) -> DshStatus {
    current_dsh_status(&state)
}

fn reserve_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("无法选择空闲端口：{error}"))?
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("无法读取空闲端口：{error}"))
}

fn dsh_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/")
}

pub fn dsh_command(port: u16) -> Command {
    let mut command = Command::new("/bin/zsh");
    command
        .args(["-lc", &format!("source \"$HOME/.nvm/nvm.sh\" 2>/dev/null || true; exec npx @deepseek-ai/dsh web --port {port} --trusted-host 127.0.0.1:{port}")])
        .env("npm_config_yes", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

fn process_group_id(pid: u32) -> i32 {
    -(pid as i32)
}

fn dsh_navigation_url(address: &str) -> url::Url {
    url::Url::parse(address).expect("DSH URL must be valid")
}

fn navigate_to_dsh(app: &AppHandle, address: &str) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_owned())?
        .navigate(dsh_navigation_url(address))
        .map_err(|error| format!("无法打开 DeepSeek Harness：{error}"))
}

fn is_exit_event(event: &RunEvent) -> bool {
    matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit)
}

pub async fn wait_for_dsh(url: &str, timeout: Duration) -> Result<(), String> {
    let client = reqwest::Client::new();
    let polling = async {
        loop {
            if let Ok(response) = client.get(url).send().await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };

    tokio::time::timeout(timeout, polling)
        .await
        .map_err(|_| format!("timed out waiting for DeepSeek Harness at {url}"))?
}

fn emit_status(app: &AppHandle, state: &'static str, message: impl Into<String>) {
    let status = DshStatus {
        state,
        message: message.into(),
    };
    *app
        .state::<AppState>()
        .status
        .lock()
        .expect("status lock poisoned") = status.clone();
    let _ = app.emit(
        "dsh-status",
        status,
    );
}

fn start_dsh(app: AppHandle) -> Result<(), String> {
    emit_status(&app, "starting", "正在启动 DeepSeek Harness…");
    let address = dsh_url(reserve_port()?);
    let child = dsh_command(address.rsplit(':').next().unwrap().trim_end_matches('/').parse().unwrap())
        .spawn()
        .map_err(|error| format!("无法启动 npx：{error}"))?;
    *app.state::<AppState>().child.lock().expect("child lock poisoned") = Some(child);

    tauri::async_runtime::spawn(async move {
        match wait_for_dsh(&address, STARTUP_TIMEOUT).await {
            Ok(()) => {
                emit_status(&app, "ready", "DeepSeek Harness 已就绪");
                if let Err(error) = navigate_to_dsh(&app, &address) {
                    emit_status(&app, "error", error);
                }
            }
            Err(error) => emit_status(&app, "error", error),
        }
    });
    Ok(())
}

fn stop_dsh(app: &AppHandle) {
    if let Some(mut child) = app
        .state::<AppState>()
        .child
        .lock()
        .expect("child lock poisoned")
        .take()
    {
        unsafe {
            libc::kill(process_group_id(child.id()), libc::SIGTERM);
        }
        let _ = child.wait();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![dsh_status])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(error) = start_dsh(handle.clone()) {
                emit_status(&handle, "error", error);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building DeepHarness");

    app.run(|app, event| {
        if is_exit_event(&event) {
            stop_dsh(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, time::Duration};

    #[test]
    fn dsh_command_uses_login_shell_to_find_npx() {
        let command = dsh_command(43127);
        assert_eq!(command.get_program(), OsStr::new("/bin/zsh"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new("-lc"), OsStr::new("source \"$HOME/.nvm/nvm.sh\" 2>/dev/null || true; exec npx @deepseek-ai/dsh web --port 43127 --trusted-host 127.0.0.1:43127")]
        );
    }

    #[test]
    fn process_group_id_targets_the_childs_entire_group() {
        assert_eq!(process_group_id(42), -42);
    }

    #[test]
    fn exit_event_triggers_dsh_cleanup() {
        assert!(is_exit_event(&RunEvent::Exit));
    }

    #[test]
    fn initial_status_is_available_after_frontend_subscribes() {
        let state = AppState::default();
        let status = current_dsh_status(&state);
        assert_eq!(status.state, "starting");
        assert_eq!(status.message, "正在准备本地服务");
    }

    #[test]
    fn navigation_url_is_the_local_dsh_server() {
        assert_eq!(dsh_navigation_url("http://127.0.0.1:43127/").port(), Some(43127));
    }

    #[tokio::test]
    async fn readiness_check_times_out_for_unreachable_server() {
        let error = wait_for_dsh("http://127.0.0.1:9/", Duration::from_millis(30))
            .await
            .unwrap_err();
        assert!(error.contains("timed out"));
    }
}
