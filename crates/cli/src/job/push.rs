use std::error::Error;
use std::path::Path;
use std::process::Command;

use super::environment::Environment;
use super::process::capture;

pub(super) const MAX_PUSH_BYTES: usize = 40 * 1024 * 1024;
const MAX_FILE_LIST_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("--push DIR must be inside a git work tree")]
struct NotWorkTree;

pub(super) fn archive(
    path: Option<&Path>,
    environment: &Environment,
) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let mut check = git(path);
    check.args(["rev-parse", "--is-inside-work-tree"]);
    let result = capture("git", check, Vec::new(), MAX_FILE_LIST_BYTES)?;
    if result != b"true\n" {
        return Err(NotWorkTree.into());
    }
    let mut listing = git(path);
    listing.args([
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ]);
    let names = capture("git", listing, Vec::new(), MAX_FILE_LIST_BYTES)?;
    let mut tar = Command::new("tar");
    tar.env("COPYFILE_DISABLE", "1");
    #[cfg(target_os = "macos")]
    tar.args([
        "--no-xattrs",
        "--no-mac-metadata",
        "--no-fflags",
        "--no-acls",
    ]);
    tar.arg("-C")
        .arg(path)
        .args(["-cf", "-", "--no-recursion", "--null", "-T", "-"]);
    let archive = capture("tar", tar, names, MAX_PUSH_BYTES)?;
    environment.reject_bytes(&archive)?;
    let mut gzip = Command::new("gzip");
    gzip.arg("-c");
    Ok(Some(capture("gzip", gzip, archive, MAX_PUSH_BYTES)?))
}

fn git(path: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-C"])
        .arg(path);
    command
}
