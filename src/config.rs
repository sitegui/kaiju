use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api_host: String,
    pub api_parallelism: usize,
    pub api_timeout_seconds: u64,
    pub email: String,
    pub token: String,
    pub server_port: u16,
    pub server_ip: String,
    pub issue_fields: Vec<IssueFieldConfig>,
    pub value_bag: BTreeMap<String, BTreeMap<String, String>>,
    pub transitions: Vec<TransitionConfig>,
    pub board: BTreeMap<String, BoardLocalConfig>,
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueFieldConfig {
    pub name: String,
    pub api_field: String,
    #[serde(flatten)]
    pub values: IssueFieldValuesConfig,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum IssueFieldValuesConfig {
    Simple { values: Vec<String> },
    FromBag { values_from: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransitionConfig {
    pub id: String,
    pub name: String,
    pub to_status: String,
    pub to_status_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardLocalConfig {
    pub board_id: String,
    pub card_avatars: Vec<String>,
    pub show_first_column: bool,
    pub filter_last_column_resolved: Option<String>,
    pub epic_short_name: String,
    pub epic_color: Option<String>,
    pub flag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub ttl_board_configuration_seconds: u64,
    pub ttl_board_issues_seconds: u64,
    pub ttl_issue_seconds: u64,
    pub ttl_epic_seconds: u64,
    pub ttl_development_info_seconds: u64,
}

const DEFAULT_CONFIG: &str = include_str!("../resources/default_config.toml");

impl Config {
    pub fn default_path(project_dirs: &ProjectDirs) -> Result<PathBuf> {
        Ok(project_dirs.config_dir().join("config.toml"))
    }

    pub fn read_contents(project_dirs: &ProjectDirs) -> Result<String> {
        let path = Config::default_path(project_dirs)?;

        match fs::read_to_string(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(DEFAULT_CONFIG.to_string()),
            Ok(contents) => Ok(contents),
            error => error.context("Could not read config file"),
        }
    }

    pub fn write_contents(project_dirs: &ProjectDirs, contents: String) -> Result<()> {
        tracing::debug!("Will save {:?}", contents);

        if let Err(error) = toml::from_str::<Config>(&contents) {
            tracing::warn!(
                "The new contents of the config file seem invalid: {}",
                error
            );
        }

        let path = Config::default_path(project_dirs)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        tracing::info!("Will update {}", path.display());
        fs::write(path, contents)?;

        Ok(())
    }

    pub fn new(project_dirs: &ProjectDirs) -> Result<Self> {
        let config: Config = toml::from_str(&Config::read_contents(project_dirs)?)?;

        tracing::debug!("Loaded config {:?}", config);

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default() {
        toml::from_str::<Config>(DEFAULT_CONFIG).unwrap();
    }
}
