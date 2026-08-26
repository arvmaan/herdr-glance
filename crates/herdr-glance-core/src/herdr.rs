use crate::config::{ConfigError, ConnectionConfig};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentView {
    pub pane_id: String,
    pub workspace: String,
    pub tab: String,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub focused: bool,
}

#[derive(Debug, Error)]
pub enum HerdrError {
    #[error("{0}")]
    Config(#[from] ConfigError),
    #[error("Herdr command could not start: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Herdr command timed out")]
    Timeout,
    #[error("Herdr command failed: {0}")]
    Command(String),
    #[error("Herdr returned invalid JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("Herdr returned an invalid session snapshot")]
    Snapshot,
}

pub async fn list_agents(config: &ConnectionConfig) -> Result<Vec<AgentView>, HerdrError> {
    let output = run_herdr(config, &["api", "snapshot"]).await?;
    let payload: Value = serde_json::from_slice(&output).map_err(HerdrError::Json)?;
    parse_snapshot(&payload)
}

pub async fn focus_agent(config: &ConnectionConfig, pane_id: &str) -> Result<(), HerdrError> {
    validate_pane_id(pane_id)?;
    run_herdr(config, &["agent", "focus", pane_id])
        .await
        .map(|_| ())
}

pub fn agent_attach_shell_command(
    config: &ConnectionConfig,
    pane_id: &str,
) -> Result<String, HerdrError> {
    config.validate()?;
    validate_pane_id(pane_id)?;

    if config.uses_ssh() {
        let remote = remote_command(
            &config.remote_herdr,
            &["agent", "attach", pane_id, "--takeover"],
        );
        Ok(format!(
            "ssh -t -o ConnectTimeout=5 -o LogLevel=ERROR {} {}",
            shell_quote(&config.ssh_target),
            shell_quote(&remote)
        ))
    } else {
        let binary = local_herdr_binary(config);
        let binary = binary.to_str().ok_or_else(|| {
            HerdrError::Command("Herdr executable path is not valid UTF-8.".to_string())
        })?;
        Ok(format!(
            "{} agent attach {} --takeover",
            shell_quote(binary),
            shell_quote(pane_id)
        ))
    }
}

fn validate_pane_id(pane_id: &str) -> Result<(), HerdrError> {
    if pane_id.is_empty() || pane_id.chars().any(char::is_control) {
        return Err(HerdrError::Command("invalid pane identifier".to_string()));
    }
    Ok(())
}

async fn run_herdr(config: &ConnectionConfig, arguments: &[&str]) -> Result<Vec<u8>, HerdrError> {
    config.validate()?;
    let mut command = if config.uses_ssh() {
        let remote_command = remote_command(&config.remote_herdr, arguments);
        let mut command = Command::new("ssh");
        command.args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "LogLevel=ERROR",
            &config.ssh_target,
            &remote_command,
        ]);
        command
    } else {
        let mut command = Command::new(local_herdr_binary(config));
        command.args(arguments);
        command
    };
    command.stdin(Stdio::null()).kill_on_drop(true);

    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| HerdrError::Timeout)?
        .map_err(HerdrError::Spawn)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if config.uses_ssh() && stderr.contains("command not found") {
            return Err(HerdrError::Command(
                "Herdr was not found on the SSH host. Enter its absolute executable path in Connection settings."
                    .to_string(),
            ));
        }
        return Err(HerdrError::Command(if stderr.is_empty() {
            format!("process exited with {}", output.status)
        } else {
            stderr
        }));
    }
    Ok(output.stdout)
}

fn local_herdr_binary(config: &ConnectionConfig) -> OsString {
    if config.remote_herdr == "herdr" {
        std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| OsString::from("herdr"))
    } else {
        OsString::from(&config.remote_herdr)
    }
}

fn remote_command(binary: &str, arguments: &[&str]) -> String {
    let command = std::iter::once(binary)
        .chain(arguments.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    format!("PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/bin:$PATH\" {command}")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._:-".contains(character))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn parse_snapshot(payload: &Value) -> Result<Vec<AgentView>, HerdrError> {
    let snapshot = payload
        .pointer("/result/snapshot")
        .and_then(Value::as_object)
        .ok_or(HerdrError::Snapshot)?;

    let workspaces = labels_by_id(snapshot.get("workspaces"), "workspace_id");
    let tabs = labels_by_id(snapshot.get("tabs"), "tab_id");
    let agents = snapshot
        .get("agents")
        .and_then(Value::as_array)
        .ok_or(HerdrError::Snapshot)?;

    let mut result = Vec::new();
    for raw in agents {
        let Some(agent) = raw.as_object() else {
            continue;
        };
        let pane_id = string(agent.get("pane_id"));
        if pane_id.is_empty() {
            continue;
        }

        let workspace_id = string(agent.get("workspace_id"));
        let tab_id = string(agent.get("tab_id"));
        let workspace = workspaces
            .get(&workspace_id)
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| workspace_id.clone());
        let tab = tabs
            .get(&tab_id)
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or(tab_id);

        let status = match string(agent.get("agent_status")).as_str() {
            "working" => "working",
            "blocked" => "blocked",
            "idle" => "idle",
            "done" => "done",
            _ => "unknown",
        }
        .to_string();
        let kind =
            first_string(agent, &["display_agent", "agent"]).unwrap_or_else(|| "agent".to_string());
        let context = if workspace.is_empty() {
            pane_id.clone()
        } else {
            workspace.clone()
        };
        let name = format!("{context} - {kind}");

        result.push(AgentView {
            pane_id,
            workspace,
            tab,
            name,
            kind,
            status,
            focused: agent
                .get("focused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }

    result.sort_by(|left, right| {
        status_priority(&left.status)
            .cmp(&status_priority(&right.status))
            .then_with(|| {
                left.workspace
                    .to_lowercase()
                    .cmp(&right.workspace.to_lowercase())
            })
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    Ok(result)
}

fn labels_by_id(value: Option<&Value>, id_key: &str) -> HashMap<String, String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|item| (string(item.get(id_key)), string(item.get("label"))))
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

fn first_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .map(|key| string(object.get(*key)))
        .find(|value| !value.is_empty())
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn status_priority(status: &str) -> u8 {
    match status {
        "blocked" => 0,
        "working" => 1,
        "idle" => 2,
        "done" => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_sorts_agents() {
        let payload = json!({
            "result": {
                "snapshot": {
                    "workspaces": [
                        {"workspace_id": "w1", "label": "Glance"},
                        {"workspace_id": "w2", "label": "API"}
                    ],
                    "tabs": [
                        {"tab_id": "w1:t1", "label": "UI"},
                        {"tab_id": "w2:t1", "label": "Research"}
                    ],
                    "agents": [
                        {
                            "agent": "codex",
                            "agent_status": "working",
                            "name": "account-name",
                            "pane_id": "w1:p1",
                            "workspace_id": "w1",
                            "tab_id": "w1:t1",
                            "focused": true
                        },
                        {
                            "agent": "claude",
                            "agent_status": "blocked",
                            "terminal_title_stripped": "socket contract",
                            "pane_id": "w2:p1",
                            "workspace_id": "w2",
                            "tab_id": "w2:t1",
                            "focused": false
                        }
                    ]
                }
            }
        });

        let agents = parse_snapshot(&payload).unwrap();
        assert_eq!(agents[0].status, "blocked");
        assert_eq!(agents[0].name, "API - claude");
        assert_eq!(agents[0].workspace, "API");
        assert_eq!(agents[1].status, "working");
        assert_eq!(agents[1].name, "Glance - codex");
        assert!(agents[1].focused);
    }

    #[test]
    fn rejects_malformed_snapshot() {
        assert!(matches!(
            parse_snapshot(&json!({"result": null})),
            Err(HerdrError::Snapshot)
        ));
    }

    #[test]
    fn names_agents_from_workspace_and_kind() {
        let payload = json!({
            "result": {
                "snapshot": {
                    "workspaces": [{"workspace_id": "w1", "label": "glance"}],
                    "tabs": [{"tab_id": "w1:t1", "label": "1"}],
                    "agents": [{
                        "agent": "codex",
                        "agent_status": "working",
                        "name": "account-name",
                        "terminal_title_stripped": "account-name",
                        "pane_id": "w1:p1",
                        "workspace_id": "w1",
                        "tab_id": "w1:t1"
                    }]
                }
            }
        });

        let agents = parse_snapshot(&payload).unwrap();
        assert_eq!(agents[0].name, "glance - codex");
    }

    #[test]
    fn quotes_remote_arguments() {
        assert_eq!(
            remote_command("/Applications/My Herdr/herdr", &["agent", "focus", "w1:p1"]),
            "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/bin:$PATH\" '/Applications/My Herdr/herdr' agent focus w1:p1"
        );
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn builds_local_attach_command() {
        let config = ConnectionConfig {
            ssh_target: String::new(),
            remote_herdr: "/Applications/My Herdr/herdr".to_string(),
        };

        assert_eq!(
            agent_attach_shell_command(&config, "w1:p1").unwrap(),
            "'/Applications/My Herdr/herdr' agent attach w1:p1 --takeover"
        );
    }

    #[test]
    fn builds_ssh_attach_command() {
        let config = ConnectionConfig {
            ssh_target: "herdr-host".to_string(),
            remote_herdr: "/home/me/My Herdr/herdr".to_string(),
        };

        assert_eq!(
            agent_attach_shell_command(&config, "w1:p1").unwrap(),
            "ssh -t -o ConnectTimeout=5 -o LogLevel=ERROR herdr-host 'PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/bin:$PATH\" '\"'\"'/home/me/My Herdr/herdr'\"'\"' agent attach w1:p1 --takeover'"
        );
    }
}
