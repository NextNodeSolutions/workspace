use std::path::{Path, PathBuf};
use std::{fs, io};

#[derive(Debug, thiserror::Error)]
#[error("cannot {action} {}", path.display())]
pub struct FileError {
    action: &'static str,
    path: PathBuf,
    #[source]
    source: io::Error,
}

pub fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> FileError {
    let path = path.to_owned();
    move |source| FileError {
        action,
        path,
        source,
    }
}

pub fn read_if_exists(path: &Path) -> Result<Option<String>, FileError> {
    match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        text => text.map(Some).map_err(io_error("read", path)),
    }
}
