//! What the config file and the records share: a missing file is often no
//! error, and every other failure names the file and the action.

use std::path::{Path, PathBuf};
use std::{fs, io};

/// A file or directory that could not be read, written, created or removed.
#[derive(Debug, thiserror::Error)]
#[error("cannot {action} {}", path.display())]
pub struct FileError {
    action: &'static str,
    path: PathBuf,
    #[source]
    source: io::Error,
}

/// Turns a failure to `action` the file at `path` into a [`FileError`].
pub fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> FileError {
    let path = path.to_owned();
    move |source| FileError {
        action,
        path,
        source,
    }
}

/// The text of the file at `path`, or `None` when it is missing.
///
/// # Errors
///
/// When the file exists but cannot be read as text.
pub fn read_if_exists(path: &Path) -> Result<Option<String>, FileError> {
    match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        text => text.map(Some).map_err(io_error("read", path)),
    }
}
