use herdr_glance_core::{
    focus_agent as focus_remote_agent, list_agents as list_remote_agents,
    save_config as persist_config, AgentView, ConnectionConfig,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{LogicalSize, State, WebviewWindow};

pub struct AppState {
    config: Mutex<ConnectionConfig>,
    configured: Mutex<bool>,
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

#[tauri::command]
pub async fn focus_agent(pane_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config()?;
    focus_remote_agent(&config, &pane_id)
        .await
        .map_err(|error| error.to_string())
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
