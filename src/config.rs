use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub journal_path: Option<PathBuf>,
    pub editor: Option<String>,
    pub date_format: Option<String>,
}

impl Config {
    pub fn date_fmt(&self) -> &str {
        self.date_format.as_deref().unwrap_or("%Y-%m-%d")
    }

    /// Returns the configured journal path or an error with setup hint.
    pub fn require_journal_path(&self) -> Result<&PathBuf> {
        self.journal_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(
                "Journal not configured. Run: journal setup <path>"
            ))
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("journal")
        .join("config.toml")
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;
    toml::from_str(&contents).with_context(|| "Failed to parse config")
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(config)?;
    std::fs::write(&path, contents)?;
    Ok(())
}
