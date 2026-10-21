use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Deserialize;
use toml_edit::DocumentMut;
use workspace_provider_boat as boat;

use crate::files::{self, FileError};
use crate::xdg;

const CONFIG_FILE: &str = "workspace/config.toml";
const DEFAULT_CONFIG_HOME: &str = ".config";

const PROVIDER_KEY: &str = "provider";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The provider a command runs on when `--provider` is not given.
    pub provider: Option<String>,
    #[serde(default)]
    pub providers: Providers,
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
    #[error("cannot locate the config directory: set XDG_CONFIG_HOME or HOME")]
    NoConfigDirectory,
    #[error(transparent)]
    Io(#[from] FileError),
    #[error("{} is not a valid config file", path.display())]
    Invalid {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{} is not a valid config file, so it was left as it is", path.display())]
    Uneditable {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },
}

pub fn load(path: Option<&Path>) -> Result<Config, ConfigError> {
    let (path, is_required) = match path {
        Some(path) => (path.to_owned(), true),
        None => match default_path() {
            Some(path) => (path, false),
            None => return Ok(Config::default()),
        },
    };
    let Some(text) = read(&path, is_required)? else {
        return Ok(Config::default());
    };
    toml::from_str(&text).map_err(|source| ConfigError::Invalid { path, source })
}

/// Makes `provider` the default provider in the file at `path`, or else in
/// the default file, and returns that file's path. The file is created when
/// it is missing. The rest of it, comments included, stays as it was, since
/// people edit it by hand.
///
/// # Errors
///
/// When the file cannot be read, parsed or written.
pub fn remember_provider(path: Option<&Path>, provider: &str) -> Result<PathBuf, ConfigError> {
    let path = match path {
        Some(path) => path.to_owned(),
        None => default_path().ok_or(ConfigError::NoConfigDirectory)?,
    };
    let text = read(&path, false)?.unwrap_or_default();
    let mut document: DocumentMut = text.parse().map_err(|source| ConfigError::Uneditable {
        path: path.clone(),
        source,
    })?;
    if document.get(PROVIDER_KEY).and_then(|item| item.as_str()) == Some(provider) {
        return Ok(path);
    }
    document[PROVIDER_KEY] = toml_edit::value(provider);
    write(&path, &document.to_string())?;
    Ok(path)
}

fn default_path() -> Option<PathBuf> {
    let config_home = xdg::base_directory("XDG_CONFIG_HOME", DEFAULT_CONFIG_HOME)?;
    Some(config_home.join(CONFIG_FILE))
}

/// The text of the file at `path`, or `None` when it is missing and not
/// required.
fn read(path: &Path, is_required: bool) -> Result<Option<String>, FileError> {
    if is_required {
        return fs::read_to_string(path)
            .map(Some)
            .map_err(files::io_error("read", path));
    }
    files::read_if_exists(path)
}

/// Writes in place, so the file keeps its permissions, and a symbolic link,
/// such as a dotfile manager sets up, stays a link to the file it names.
fn write(path: &Path, text: &str) -> Result<(), FileError> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(files::io_error("create", directory))?;
    }
    fs::write(path, text).map_err(files::io_error("write", path))
}
