use std::path::{Path, PathBuf};
use std::{fs, io};

use clap::Args;
use serde::Deserialize;
use workspace_provider_boat as boat;

use crate::xdg;

const CONFIG_FILE: &str = "workspace/config.toml";
const DEFAULT_CONFIG_HOME: &str = ".config";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    providers: Providers,
}

#[derive(Args, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Providers {
    #[command(flatten)]
    #[serde(default)]
    pub boat: boat::Options,
}

impl Providers {
    pub fn or(self, fallback: Self) -> Self {
        Self {
            boat: self.boat.or(fallback.boat),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a valid config file", path.display())]
    Invalid {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub fn load(path: Option<&Path>) -> Result<Providers, ConfigError> {
    let (path, is_required) = match path {
        Some(path) => (path.to_owned(), true),
        None => match default_path() {
            Some(path) => (path, false),
            None => return Ok(Providers::default()),
        },
    };
    let text = match fs::read_to_string(&path) {
        Err(error) if !is_required && error.kind() == io::ErrorKind::NotFound => {
            return Ok(Providers::default());
        }
        text => text.map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?,
    };
    let file: File =
        toml::from_str(&text).map_err(|source| ConfigError::Invalid { path, source })?;
    Ok(file.providers)
}

fn default_path() -> Option<PathBuf> {
    let config_home = xdg::base_directory("XDG_CONFIG_HOME", DEFAULT_CONFIG_HOME)?;
    Some(config_home.join(CONFIG_FILE))
}
