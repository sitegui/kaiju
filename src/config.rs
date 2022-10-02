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

        toml::from_str::<Config>(&contents).context("The config seems invalid")?;

        let path = Config::default_path(project_dirs)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;

        Ok(())
    }

    pub fn new(project_dirs: &ProjectDirs) -> Result<Self> {
        let mut config: Config = toml::from_str(&Config::read_contents(project_dirs)?)?;

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
