use std::env;
use std::path::{Path, PathBuf};

pub fn base_directory(variable: &str, fallback: &str) -> Option<PathBuf> {
    let directory = env::var_os(variable).filter(|value| !value.is_empty());
    directory
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(fallback)))
}
