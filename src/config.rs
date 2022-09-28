use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    token: Option<String>,
}

const DEFAULT_CONFIG: &str = include_str!("../resources/default_config.toml");

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("", "sitegui", "kaiju")
            .context("Could not determine local configuration directory")?;

        Ok(project_dirs.config_dir().join("config.toml"))
    }

    pub fn read_contents() -> Result<String> {
        let path = Config::default_path()?;

        match fs::read_to_string(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(DEFAULT_CONFIG.to_string()),
            Ok(contents) => Ok(contents),
            error => error.context("Could not read config file"),
        }
    }

    pub fn write_contents(contents: String) -> Result<()> {
        tracing::debug!("Will save {:?}", contents);

        toml::from_str::<Config>(&contents).context("The config seems invalid")?;

        let path = Config::default_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;

        Ok(())
    }

    pub fn new() -> Result<Self> {
        let mut config: Config = toml::from_str(&Config::read_contents()?)?;

        if config.token.as_deref() == Some("") {
            config.token = None;
        }

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
