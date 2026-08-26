use herdr_glance_core::{
    focus_agent, herdr_session_shell_command, list_agents as list_remote_agents,
    save_config as persist_config, AgentView, ConnectionConfig,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{LogicalSize, State, WebviewWindow};
use tokio::sync::Mutex as AsyncMutex;

#[cfg(target_os = "macos")]
const GHOSTTY_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct GhosttyTerminal {
    connection_key: u64,
    terminal_id: String,
}

pub struct AppState {
    config: Mutex<ConnectionConfig>,
    configured: Mutex<bool>,
    open_lock: AsyncMutex<()>,
    #[cfg(target_os = "macos")]
    ghostty_terminal: Mutex<Option<GhosttyTerminal>>,
    startup_warning: Option<String>,
}

impl AppState {
    pub fn new(
        config: ConnectionConfig,
        configured: bool,
        startup_warning: Option<String>,
    ) -> Self {
        Self {
            config: Mutex::new(config),
            configured: Mutex::new(configured),
            open_lock: AsyncMutex::new(()),
            #[cfg(target_os = "macos")]
            ghostty_terminal: Mutex::new(None),
            startup_warning,
        }
    }

    fn config(&self) -> Result<ConnectionConfig, String> {
        self.config
            .lock()
            .map(|config| config.clone())
            .map_err(|_| "Connection settings are unavailable.".to_string())
    }

    fn configured(&self) -> Result<bool, String> {
        self.configured
            .lock()
            .map(|configured| *configured)
            .map_err(|_| "Connection settings are unavailable.".to_string())
    }

    #[cfg(target_os = "macos")]
    fn ghostty_terminal_id(&self, connection_key: u64) -> Result<Option<String>, String> {
        self.ghostty_terminal
            .lock()
            .map(|terminal| {
                terminal
                    .as_ref()
                    .filter(|terminal| terminal.connection_key == connection_key)
                    .map(|terminal| terminal.terminal_id.clone())
            })
            .map_err(|_| "Ghostty window state is unavailable.".to_string())
    }

    #[cfg(target_os = "macos")]
    fn set_ghostty_terminal(&self, connection_key: u64, terminal_id: String) -> Result<(), String> {
        self.ghostty_terminal
            .lock()
            .map(|mut current| {
                *current = Some(GhosttyTerminal {
                    connection_key,
                    terminal_id,
                })
            })
            .map_err(|_| "Ghostty window state is unavailable.".to_string())
    }
}

#[derive(Serialize)]
pub struct ConnectionBootstrap {
    config: ConnectionConfig,
    configured: bool,
    warning: Option<String>,
}

#[tauri::command]
pub fn get_connection(state: State<'_, AppState>) -> Result<ConnectionBootstrap, String> {
    Ok(ConnectionBootstrap {
        config: state.config()?,
        configured: state.configured()?,
        warning: state.startup_warning.clone(),
    })
}

#[tauri::command]
pub fn save_connection(config: ConnectionConfig, state: State<'_, AppState>) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;
    persist_config(&config).map_err(|error| error.to_string())?;
    let mut current = state
        .config
        .lock()
        .map_err(|_| "Connection settings are unavailable.".to_string())?;
    *current = config;
    let mut configured = state
        .configured
        .lock()
        .map_err(|_| "Connection settings are unavailable.".to_string())?;
    *configured = true;
    Ok(())
}

#[tauri::command]
pub async fn list_agents(state: State<'_, AppState>) -> Result<Vec<AgentView>, String> {
    let config = state.config()?;
    list_remote_agents(&config)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GhosttyAction {
    Focused,
    Opened,
}

#[tauri::command]
pub async fn open_agent_in_herdr(
    pane_id: String,
    state: State<'_, AppState>,
) -> Result<GhosttyAction, String> {
    let _open_guard = state.open_lock.lock().await;
    let config = state.config()?;
    focus_agent(&config, &pane_id)
        .await
        .map_err(|error| error.to_string())?;
    let command = herdr_session_shell_command(&config).map_err(|error| error.to_string())?;
    launch_or_focus_ghostty(&config, &command, state.inner()).await
}

#[cfg(target_os = "macos")]
async fn launch_or_focus_ghostty(
    config: &ConnectionConfig,
    command: &str,
    state: &AppState,
) -> Result<GhosttyAction, String> {
    let connection_key = ghostty_connection_key(config);
    let window_title = format!("Herdr Glance [{connection_key:016x}]");
    match focus_modern_ghostty_window(&window_title, state.ghostty_terminal_id(connection_key)?)
        .await?
    {
        ModernGhosttyWindow::Focused(terminal_id) => {
            state.set_ghostty_terminal(connection_key, terminal_id)?;
            return Ok(GhosttyAction::Focused);
        }
        ModernGhosttyWindow::Missing => {
            launch_ghostty_session(command, &window_title).await?;
            return Ok(GhosttyAction::Opened);
        }
        ModernGhosttyWindow::Unsupported => {}
    }

    if focus_legacy_ghostty_window(&window_title).await? {
        return Ok(GhosttyAction::Focused);
    }

    launch_ghostty_session(command, &window_title).await?;
    Ok(GhosttyAction::Opened)
}

#[cfg(target_os = "macos")]
enum ModernGhosttyWindow {
    Focused(String),
    Missing,
    Unsupported,
}

#[cfg(target_os = "macos")]
async fn focus_modern_ghostty_window(
    window_title: &str,
    terminal_id: Option<String>,
) -> Result<ModernGhosttyWindow, String> {
    const SCRIPT: &str = r#"
on run argv
    set markerTitle to item 1 of argv
    set savedTerminalID to item 2 of argv

    if application id "com.mitchellh.ghostty" is not running then
        return "missing"
    end if

    tell application id "com.mitchellh.ghostty"
        if savedTerminalID is not "" then
            try
                set herdrTerminal to terminal id savedTerminalID
                focus herdrTerminal
                return "focused|" & (id of herdrTerminal)
            end try
        end if

        repeat with candidate in windows
            if (name of candidate) ends with markerTitle then
                set herdrWindow to contents of candidate
                set herdrTerminal to focused terminal of selected tab of herdrWindow
                focus herdrTerminal
                return "focused|" & (id of herdrTerminal)
            end if
        end repeat
    end tell
    return "missing"
end run
"#;

    let mut command = tokio::process::Command::new("/usr/bin/osascript");
    command
        .args(["-e", SCRIPT, window_title])
        .arg(terminal_id.unwrap_or_default())
        .kill_on_drop(true);
    let output = command_output(command, "Ghostty window lookup").await?;
    if !output.status.success() {
        return Ok(ModernGhosttyWindow::Unsupported);
    }

    let result = String::from_utf8_lossy(&output.stdout);
    let result = result.trim();
    if result == "missing" {
        return Ok(ModernGhosttyWindow::Missing);
    }
    let terminal_id = result
        .strip_prefix("focused|")
        .filter(|terminal_id| !terminal_id.is_empty())
        .ok_or_else(|| "Ghostty returned an invalid window lookup result.".to_string())?;
    Ok(ModernGhosttyWindow::Focused(terminal_id.to_string()))
}

#[cfg(target_os = "macos")]
async fn focus_legacy_ghostty_window(window_title: &str) -> Result<bool, String> {
    const SCRIPT: &str = r#"
on run argv
    set markerTitle to item 1 of argv
    tell application "System Events"
        set ghosttyProcesses to every application process whose bundle identifier is "com.mitchellh.ghostty"
        repeat with ghosttyProcess in ghosttyProcesses
            repeat with candidate in windows of ghosttyProcess
                try
                    if (name of candidate) ends with markerTitle then
                        set frontmost of ghosttyProcess to true
                        perform action "AXRaise" of candidate
                        return "focused"
                    end if
                end try
            end repeat
        end repeat
    end tell
    return "missing"
end run
"#;

    let mut command = tokio::process::Command::new("/usr/bin/osascript");
    command
        .args(["-e", SCRIPT, window_title])
        .kill_on_drop(true);
    let output = command_output(command, "macOS Ghostty window lookup").await?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim() == "focused");
    }

    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(" ({detail})")
    };
    Err(format!(
        "Allow Herdr Glance to control Ghostty in System Settings > Privacy & Security > Accessibility, then try again.{suffix}"
    ))
}

#[cfg(target_os = "macos")]
async fn launch_ghostty_session(command: &str, window_title: &str) -> Result<(), String> {
    let title_argument = format!("--title={window_title}");
    let mut process = tokio::process::Command::new("/usr/bin/open");
    process
        .args([
            "-na",
            "Ghostty.app",
            "--args",
            &title_argument,
            "-e",
            "/bin/sh",
            "-c",
            command,
        ])
        .kill_on_drop(true);
    let output = command_output(process, "Opening Herdr in Ghostty").await?;
    if output.status.success() {
        return Ok(());
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if message.is_empty() {
        format!("Ghostty could not open Herdr ({}).", output.status)
    } else {
        format!("Ghostty could not open Herdr: {message}")
    })
}

#[cfg(target_os = "macos")]
async fn command_output(
    mut command: tokio::process::Command,
    description: &str,
) -> Result<std::process::Output, String> {
    tokio::time::timeout(GHOSTTY_COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| format!("{description} timed out."))?
        .map_err(|error| format!("{description} could not start: {error}"))
}

#[cfg(target_os = "macos")]
fn ghostty_connection_key(config: &ConnectionConfig) -> u64 {
    // FNV-1a keeps the title stable without exposing connection details.
    let mut hash = 0xcbf29ce484222325_u64;
    let mode = if config.uses_ssh() { "ssh" } else { "local" };
    for value in [mode, config.ssh_target.trim(), config.remote_herdr.trim()] {
        for byte in value.bytes().chain(std::iter::once(0xff)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(not(target_os = "macos"))]
async fn launch_or_focus_ghostty(
    _config: &ConnectionConfig,
    _command: &str,
    _state: &AppState,
) -> Result<GhosttyAction, String> {
    Err("Opening Herdr in Ghostty is currently supported on macOS.".to_string())
}

#[tauri::command]
pub async fn test_connection(config: ConnectionConfig) -> Result<usize, String> {
    list_remote_agents(&config)
        .await
        .map(|agents| agents.len())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn resize_window(width: f64, height: f64, window: WebviewWindow) -> Result<(), String> {
    if !(180.0..=480.0).contains(&width) || !(35.0..=600.0).contains(&height) {
        return Err("Requested widget size is outside the supported range.".to_string());
    }
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())
}
