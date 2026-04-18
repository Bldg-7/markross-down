use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::plugin::{default_plugins, PluginDef};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Plugins defined by the user. Merged with the built-in roster:
    /// user entries take precedence when the `trigger` matches a built-in.
    #[serde(default, rename = "plugins")]
    pub plugins: Vec<PluginDef>,
}

pub struct LoadedConfig {
    pub config: Config,
    pub source: ConfigSource,
}

pub enum ConfigSource {
    File(PathBuf),
    Defaults,
    Error { path: PathBuf, message: String },
}

/// Load the user's config from the platform's XDG (or equivalent) config
/// directory. Missing file → defaults. Malformed file → defaults + a message
/// so the user can see why their config was ignored.
pub fn load() -> LoadedConfig {
    let Some(dirs) = ProjectDirs::from("", "Bldg-7", "markross-down") else {
        return LoadedConfig {
            config: Config::default(),
            source: ConfigSource::Defaults,
        };
    };
    let path = dirs.config_dir().join("config.toml");
    if !path.exists() {
        return LoadedConfig {
            config: Config::default(),
            source: ConfigSource::Defaults,
        };
    }
    match read_and_parse(&path) {
        Ok(config) => LoadedConfig {
            config,
            source: ConfigSource::File(path),
        },
        Err(e) => LoadedConfig {
            config: Config::default(),
            source: ConfigSource::Error {
                path,
                message: e.to_string(),
            },
        },
    }
}

fn read_and_parse(path: &std::path::Path) -> Result<Config> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(config)
}

/// Merge user plugins with built-in defaults. User entries override built-ins
/// that share the same `trigger`; remaining user plugins are appended.
pub fn resolve_plugins(user: &[PluginDef]) -> Vec<PluginDef> {
    let mut result: Vec<PluginDef> = default_plugins();
    for user_plugin in user {
        if let Some(existing) = result.iter_mut().find(|p| p.trigger == user_plugin.trigger) {
            *existing = user_plugin.clone();
        } else {
            result.push(user_plugin.clone());
        }
    }
    result
}
