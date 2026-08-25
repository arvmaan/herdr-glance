use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ConnectionConfig {
    pub ssh_target: String,
    pub remote_herdr: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            ssh_target: String::new(),
            remote_herdr: "herdr".to_string(),
        }
    }
}

impl ConnectionConfig {
    pub fn uses_ssh(&self) -> bool {
        !self.ssh_target.trim().is_empty()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let target = self.ssh_target.trim();
        if self.uses_ssh() && (target.starts_with('-') || target.chars().any(char::is_control)) {
            return Err(ConfigError::Invalid(
                "SSH host contains unsupported characters.".to_string(),
            ));
        }

        let binary = self.remote_herdr.trim();
        if binary.is_empty() || binary.chars().any(char::is_control) {
            return Err(ConfigError::Invalid(
                "Enter the Herdr executable name or path.".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(String),
    #[error("could not read configuration: {0}")]
    Read(#[source] io::Error),
    #[error("configuration is not valid JSON: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("could not write configuration: {0}")]
    Write(#[source] io::Error),
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("herdr-glance")
        .join("config.json")
}

fn legacy_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("herdr-pills")
        .join("config.json")
}

pub fn config_exists() -> bool {
    config_path().is_file() || legacy_config_path().is_file()
}

pub fn load_config() -> Result<ConnectionConfig, ConfigError> {
    let current = config_path();
    let path = if current.exists() {
        current
    } else {
        legacy_config_path()
    };
    if !path.exists() {
        return Ok(ConnectionConfig::default());
    }
    let raw = fs::read_to_string(path).map_err(ConfigError::Read)?;
    serde_json::from_str(&raw).map_err(ConfigError::Parse)
}

pub fn save_config(config: &ConnectionConfig) -> Result<(), ConfigError> {
    config.validate()?;
    let path = config_path();
    let parent = path.parent().ok_or_else(|| {
        ConfigError::Write(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configuration path has no parent",
        ))
    })?;
    fs::create_dir_all(parent).map_err(ConfigError::Write)?;

    let serialized = serde_json::to_string_pretty(config).map_err(ConfigError::Parse)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{serialized}\n")).map_err(ConfigError::Write)?;
    fs::rename(temporary, path).map_err(ConfigError::Write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_herdr_on_path() {
        assert_eq!(ConnectionConfig::default().remote_herdr, "herdr");
    }

    #[test]
    fn validates_remote_connection() {
        let config = ConnectionConfig {
            ssh_target: "devbox".to_string(),
            remote_herdr: "/home/me/.local/bin/herdr".to_string(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validates_local_connection() {
        let config = ConnectionConfig::default();
        assert!(!config.uses_ssh());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_ssh_option_injection() {
        let config = ConnectionConfig {
            ssh_target: "-oProxyCommand=bad".to_string(),
            remote_herdr: "herdr".to_string(),
        };
        assert!(config.validate().is_err());
    }
}
