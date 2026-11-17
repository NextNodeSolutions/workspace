use std::error::Error;
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{MAX_FILE_LIST_BYTES, PushError};
use crate::job::process::capture;

pub(super) struct Source {
    git_directory: PathBuf,
    commit: String,
    scope: Vec<u8>,
}

impl Source {
    pub(super) fn resolve(path: &Path) -> Result<Self, Box<dyn Error>> {
        let path = path.canonicalize().map_err(|_| PushError::NotWorkTree)?;
        let root = location(&path, "--show-toplevel")?;
        let root = root.canonicalize().map_err(|_| PushError::NotWorkTree)?;
        let scope = path
            .strip_prefix(&root)
            .map_err(|_| PushError::NotWorkTree)?;
        let mut source = Self {
            git_directory: location(&root, "--absolute-git-dir")?,
            commit: String::new(),
            scope: scope.as_os_str().as_bytes().to_vec(),
        };
        let commit = source.run(
            &["rev-parse", "--verify", "HEAD^{commit}"],
            Vec::new(),
            MAX_FILE_LIST_BYTES,
        )?;
        source.commit = String::from_utf8(line(commit)?).map_err(|_| PushError::InvalidObject)?;
        if source.scope_type()? != b"tree" {
            return Err(PushError::NotWorkTree.into());
        }
        Ok(source)
    }

    pub(super) fn commit(&self) -> &str {
        &self.commit
    }

    pub(super) fn scope(&self) -> &[u8] {
        &self.scope
    }

    pub(super) fn listing(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut command = self.git();
        command.args(["ls-tree", "-r", "-t", "-z", "--full-tree"]);
        command.arg(self.selector());
        Ok(capture("git", command, Vec::new(), MAX_FILE_LIST_BYTES)?)
    }

    fn scope_type(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.scope.is_empty() {
            return Ok(b"tree".to_vec());
        }
        let mut command = self.git();
        command.arg("cat-file").arg("-t").arg(self.selector());
        let kind = capture("git", command, Vec::new(), MAX_FILE_LIST_BYTES)?;
        line(kind).map_err(Into::into)
    }

    fn selector(&self) -> OsString {
        let mut selector = self.commit.as_bytes().to_vec();
        selector.push(b':');
        selector.extend_from_slice(&self.scope);
        OsString::from_vec(selector)
    }

    pub(super) fn run(
        &self,
        arguments: &[&str],
        stdin: Vec<u8>,
        limit: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut command = self.git();
        command.args(arguments);
        Ok(capture("git", command, stdin, limit)?)
    }

    fn git(&self) -> Command {
        let mut command = git();
        command.arg("--git-dir").arg(&self.git_directory);
        command
    }
}

fn location(path: &Path, option: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut command = git();
    command.arg("-C").arg(path).args(["rev-parse", option]);
    let bytes = capture("git", command, Vec::new(), MAX_FILE_LIST_BYTES)?;
    Ok(PathBuf::from(OsString::from_vec(line(bytes)?)))
}

fn line(mut bytes: Vec<u8>) -> Result<Vec<u8>, PushError> {
    if bytes.pop() != Some(b'\n') || bytes.is_empty() || bytes.contains(&0) {
        return Err(PushError::InvalidObject);
    }
    Ok(bytes)
}

fn git() -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_NAMESPACE")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"]);
    command
}
