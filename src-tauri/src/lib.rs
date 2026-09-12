use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdout, Command, Stdio},
    sync::Mutex,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tokio::sync::oneshot;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(45);
/// How often the fallback probe re-checks a DSH revision that prints no token URL.
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// The loopback literal `dsh web` binds and prints for this launcher.
const LOOPBACK_HOST: &str = "127.0.0.1";
/// Query parameter carrying the per-process launch token in the printed URL.
const TOKEN_QUERY: &str = "token";

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
    std::net::TcpListener::bind((LOOPBACK_HOST, 0))
        .map_err(|error| format!("无法选择空闲端口：{error}"))?
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("无法读取空闲端口：{error}"))
}

fn dsh_url(port: u16) -> String {
    format!("http://{LOOPBACK_HOST}:{port}/")
}

#[cfg(unix)]
mod platform {
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};

    /// 把子进程放进独立进程组，便于整组终止。
    pub fn configure_command(command: &mut Command) {
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    /// 向整个进程组发送 SIGTERM。
    pub fn stop_process_group(child: &mut Child) {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGTERM);
        }
        let _ = child.wait();
    }
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};

    /// 子进程拥有独立进程组（与 Unix 的 setpgid 对应）。
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    /// 不创建控制台窗口，避免启动时弹出终端。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn configure_command(command: &mut Command) {
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    /// 用 taskkill 递归终止子进程整棵树。
    pub fn stop_process_group(child: &mut Child) {
        let pid = child.id();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
        let _ = child.wait();
    }
}

fn dsh_command(port: u16) -> Command {
    let mut command = dsh_launch_command(port);
    platform::configure_command(&mut command);
    command
}

#[cfg(unix)]
fn dsh_launch_command(port: u16) -> Command {
    let mut command = Command::new("/bin/zsh");
    command
        .args(["-lc", &format!("source \"$HOME/.nvm/nvm.sh\" 2>/dev/null || true; exec npx @deepseek-ai/dsh web --port {port} --trusted-host {LOOPBACK_HOST}:{port} --no-open")])
        .env("npm_config_yes", "true")
        .stdin(Stdio::null())
        // Piped so the launcher can read the authenticated URL `dsh web` prints.
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

#[cfg(windows)]
fn dsh_launch_command(port: u16) -> Command {
    let mut command = Command::new("cmd");
    command
        .args([
            "/C",
                &format!(
                    "npx @deepseek-ai/dsh web --port {port} --trusted-host {LOOPBACK_HOST}:{port} --no-open"
                ),
        ])
        .env("npm_config_yes", "true")
        .stdin(Stdio::null())
        // Piped so the launcher can read the authenticated URL `dsh web` prints.
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

fn dsh_navigation_url(address: &str) -> Result<url::Url, String> {
    url::Url::parse(address)
        .map_err(|error| format!("无法打开 DeepSeek Harness：地址 {address} 无效（{error}）"))
}

/// URLs the window is walked through to obtain a browser session, in order.
///
/// DSH hands out its session cookie with `SameSite=Strict` on the 303 that
/// follows `GET /?token=…`. WebKit attaches the *current document* as the
/// initiator of an app-initiated navigation, so navigating straight from the
/// launcher page (`tauri://localhost` in a bundle, `http://localhost:1420` in
/// development) into that URL makes the redirect cross-site: the freshly minted
/// Strict cookie is **not** sent on the follow-up request for `/`, and the
/// window renders DSH's raw `dsh web authentication required` page. Clearing the
/// document first removes the initiator, so the exchange succeeds on the first
/// try; the trailing load of the bare origin is same-site and therefore both a
/// harmless refresh and the recovery path when a WebKit build still withholds
/// the cookie.
fn navigation_sequence(endpoint: &str, base_url: &str) -> [String; 3] {
    [
        "about:blank".to_owned(),
        endpoint.to_owned(),
        base_url.to_owned(),
    ]
}

/// Delay before opening the authenticated URL, so `about:blank` has committed.
const INITIATOR_CLEAR_DELAY: Duration = Duration::from_millis(200);
/// Delay before the confirming same-site load of the bare origin.
const SESSION_CONFIRM_DELAY: Duration = Duration::from_millis(1200);

async fn open_dsh_window(app: &AppHandle, endpoint: &str, base_url: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_owned())?;

    let [blank, authenticated, bare] = navigation_sequence(endpoint, base_url);

    // Best effort: a build that refuses `about:blank` still recovers through the
    // same-site load below.
    if let Ok(url) = dsh_navigation_url(&blank) {
        let _ = window.navigate(url);
        tokio::time::sleep(INITIATOR_CLEAR_DELAY).await;
    }

    window
        .navigate(dsh_navigation_url(&authenticated)?)
        .map_err(|error| format!("无法打开 DeepSeek Harness：{error}"))?;

    tokio::time::sleep(SESSION_CONFIRM_DELAY).await;
    window
        .navigate(dsh_navigation_url(&bare)?)
        .map_err(|error| format!("无法打开 DeepSeek Harness：{error}"))?;

    Ok(())
}

fn is_exit_event(event: &RunEvent) -> bool {
    matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit)
}

/// Read the authenticated URL out of one line of `dsh web` output.
///
/// DSH mints a launch token per process and announces it as the only way into
/// the Web GUI:
///
/// ```text
/// dsh web: http://127.0.0.1:43127/?token=<secret> (LAN: http://192.168.1.4:43127/?token=<secret>)
/// ```
///
/// A bare `GET /` without that token (or the cookie it mints) now answers 401,
/// so the launcher must open exactly this URL. Only the loopback origin we
/// spawned is accepted: the LAN candidate on the same line is ignored, and a
/// URL for another port is refused so stray output can never redirect the
/// desktop window somewhere else.
fn parse_launch_url(line: &str, port: u16) -> Option<String> {
    line.split_whitespace().find_map(|word| {
        // The announcement is prose, so the trailing `)` of the LAN note sticks
        // to the URL we want.
        let candidate = word.trim_end_matches([')', ',', '.']);
        let url = url::Url::parse(candidate).ok()?;
        if url.scheme() != "http" || url.host_str() != Some(LOOPBACK_HOST) {
            return None;
        }
        if url.port() != Some(port) {
            return None;
        }
        let (_, token) = url.query_pairs().find(|(key, _)| key == TOKEN_QUERY)?;
        if token.is_empty() {
            return None;
        }
        Some(url.to_string())
    })
}

/// Drain `dsh web`'s stdout and hand over the authenticated URL it announces.
///
/// The reader must keep consuming the pipe for the whole process lifetime: an
/// unread pipe eventually blocks the child once the buffer fills.
fn spawn_launch_url_reader(stdout: ChildStdout, port: u16) -> oneshot::Receiver<String> {
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        let mut sender = Some(sender);
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(url) = parse_launch_url(&line, port) {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(url);
                }
            }
        }
    });
    receiver
}

/// Whether the bare origin already serves the Web GUI.
///
/// This is the readiness signal of DSH revisions without browser
/// authentication; token-guarded revisions answer 401 here and are reported
/// through the announcement channel instead.
async fn serves_web_gui(client: &reqwest::Client, url: &str) -> bool {
    match client.get(url).send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Resolve the URL the window must open: the announced token URL when
/// `dsh web` prints one, else the bare origin once it answers.
pub async fn wait_for_endpoint(
    base_url: &str,
    announced: Option<oneshot::Receiver<String>>,
    timeout: Duration,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let mut announced = announced;
    let mut announcement_pending = announced.is_some();
    let mut poll = tokio::time::interval(READINESS_POLL_INTERVAL);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            result = async { announced.as_mut().expect("announcement pending").await }, if announcement_pending => {
                announcement_pending = false;
                if let Ok(url) = result {
                    return Ok(url);
                }
            }
            _ = poll.tick() => {
                if serves_web_gui(&client, base_url).await {
                    return Ok(base_url.to_owned());
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(format!(
                    "等待 DeepSeek Harness 就绪超时（{base_url}）。请确认终端中 `npx @deepseek-ai/dsh web` 能正常启动并打印访问地址。"
                ));
            }
        }
    }
}

fn emit_status(app: &AppHandle, state: &'static str, message: impl Into<String>) {
    let status = DshStatus {
        state,
        message: message.into(),
    };
    *app.state::<AppState>()
        .status
        .lock()
        .expect("status lock poisoned") = status.clone();
    let _ = app.emit("dsh-status", status);
}

fn start_dsh(app: AppHandle) -> Result<(), String> {
    emit_status(&app, "starting", "正在启动 DeepSeek Harness…");
    let port = reserve_port()?;
    let base_url = dsh_url(port);
    let mut child = dsh_command(port)
        .spawn()
        .map_err(|error| format!("无法启动 npx：{error}"))?;
    let announced = child
        .stdout
        .take()
        .map(|stdout| spawn_launch_url_reader(stdout, port));
    *app.state::<AppState>()
        .child
        .lock()
        .expect("child lock poisoned") = Some(child);

    tauri::async_runtime::spawn(async move {
        let endpoint = wait_for_endpoint(&base_url, announced, STARTUP_TIMEOUT).await;

        match endpoint {
            Ok(address) => {
                emit_status(&app, "ready", "DeepSeek Harness 已就绪");
                if let Err(error) = open_dsh_window(&app, &address, &base_url).await {
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
        platform::stop_process_group(&mut child);
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

    #[cfg(unix)]
    #[test]
    fn dsh_command_uses_login_shell_to_find_npx() {
        let command = dsh_command(43127);
        assert_eq!(command.get_program(), OsStr::new("/bin/zsh"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new("-lc"), OsStr::new("source \"$HOME/.nvm/nvm.sh\" 2>/dev/null || true; exec npx @deepseek-ai/dsh web --port 43127 --trusted-host 127.0.0.1:43127 --no-open")]
        );
    }

    #[cfg(windows)]
    #[test]
    fn dsh_command_runs_npx_through_cmd() {
        let command = dsh_command(43127);
        assert_eq!(command.get_program(), OsStr::new("cmd"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("/C"),
                OsStr::new("npx @deepseek-ai/dsh web --port 43127 --trusted-host 127.0.0.1:43127 --no-open")
            ]
        );
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
        assert_eq!(
            dsh_navigation_url("http://127.0.0.1:43127/")
                .expect("valid")
                .port(),
            Some(43127)
        );
    }

    #[test]
    fn navigation_url_keeps_the_launch_token() {
        let url = dsh_navigation_url(
            "http://127.0.0.1:43127/?token=_ibOyVQb5jrgSQlct4_tk-jwZmHF9dCJyRnZUSFJmLU",
        )
        .expect("valid");

        assert_eq!(url.path(), "/");
        assert_eq!(
            url.query(),
            Some("token=_ibOyVQb5jrgSQlct4_tk-jwZmHF9dCJyRnZUSFJmLU")
        );
    }

    #[test]
    fn navigation_clears_the_initiator_before_using_the_token() {
        let token_url = "http://127.0.0.1:43127/?token=abc123";
        let sequence = navigation_sequence(token_url, "http://127.0.0.1:43127/");

        assert_eq!(sequence[0], "about:blank");
        assert_eq!(sequence[1], token_url);
        // The last hop must be the bare origin: a same-site load that both
        // refreshes a live session and recovers one whose cookie WebKit
        // withheld on the cross-site redirect.
        assert_eq!(sequence[2], "http://127.0.0.1:43127/");
        for step in sequence {
            assert!(dsh_navigation_url(&step).is_ok(), "{step} must parse");
        }
    }

    #[test]
    fn navigation_url_rejects_garbage() {
        assert!(dsh_navigation_url("not a url").is_err());
    }

    #[tokio::test]
    async fn readiness_check_times_out_for_unreachable_server() {
        let (_sender, announced) = oneshot::channel();
        let error = wait_for_endpoint(
            "http://127.0.0.1:9/",
            Some(announced),
            Duration::from_millis(30),
        )
        .await
        .unwrap_err();
        assert!(error.contains("超时"));
    }

    #[tokio::test]
    async fn readiness_check_falls_back_when_no_url_is_announced() {
        let error = wait_for_endpoint("http://127.0.0.1:9/", None, Duration::from_millis(30))
            .await
            .unwrap_err();
        assert!(error.contains("超时"));
    }

    #[tokio::test]
    async fn announced_token_url_wins_over_the_bare_origin() {
        let (sender, announced) = oneshot::channel();
        sender
            .send("http://127.0.0.1:43127/?token=abc123".to_owned())
            .expect("receiver is alive");
        assert_eq!(
            wait_for_endpoint(
                "http://127.0.0.1:43127/",
                Some(announced),
                Duration::from_secs(1)
            )
            .await
            .unwrap(),
            "http://127.0.0.1:43127/?token=abc123"
        );
    }

    #[test]
    fn launch_url_is_read_from_the_announcement_line() {
        let line = "dsh web: http://127.0.0.1:43127/?token=_ibOyVQb5jrgSQlct4_tk-jwZmHF9dCJyRnZUSFJmLU (LAN: http://192.168.1.4:43127/?token=_ibOyVQb5jrgSQlct4_tk-jwZmHF9dCJyRnZUSFJmLU)";
        assert_eq!(
            parse_launch_url(line, 43127).as_deref(),
            Some("http://127.0.0.1:43127/?token=_ibOyVQb5jrgSQlct4_tk-jwZmHF9dCJyRnZUSFJmLU")
        );
    }

    #[test]
    fn launch_url_ignores_unrelated_output() {
        assert_eq!(
            parse_launch_url(
                "dsh web: opening the default browser; pass --no-open to disable",
                43127
            ),
            None
        );
        assert_eq!(parse_launch_url("", 43127), None);
        // A bare origin is not an authenticated URL.
        assert_eq!(
            parse_launch_url("dsh web: http://127.0.0.1:43127/", 43127),
            None
        );
        // The LAN candidate and any other authority must not move the window.
        assert_eq!(
            parse_launch_url("http://192.168.1.4:43127/?token=abc", 43127),
            None
        );
        // A token URL for another port is refused.
        assert_eq!(
            parse_launch_url("http://127.0.0.1:3080/?token=abc", 43127),
            None
        );
        // An empty token is not a credential.
        assert_eq!(
            parse_launch_url("http://127.0.0.1:43127/?token=", 43127),
            None
        );
    }
}
