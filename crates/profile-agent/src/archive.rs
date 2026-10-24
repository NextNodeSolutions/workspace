use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::error::AgentError;
use crate::local;

const TAR: &str = "tar";

const PACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const EXCLUDED: &[&str] = &["node_modules"];

/// macOS tar otherwise writes extended headers that GNU tar warns about.
#[cfg(target_os = "macos")]
const PLATFORM_FLAGS: &[&str] = &[
    "--no-xattrs",
    "--no-mac-metadata",
    "--no-fflags",
    "--no-acls",
];

#[cfg(not(target_os = "macos"))]
const PLATFORM_FLAGS: &[&str] = &[];

/// Symbolic links are followed, so the VM gets files where this machine has links.
pub(crate) fn pack(root: &Path, entries: &[&str]) -> Result<Vec<u8>, AgentError> {
    let present: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|entry| root.join(entry).exists())
        .collect();
    if present.is_empty() {
        return Err(AgentError::NothingToMirror(root.to_owned()));
    }
    let mut command = Command::new(TAR);
    command.args(["-czhf", "-"]).args(PLATFORM_FLAGS);
    for pattern in EXCLUDED {
        command.args(["--exclude", pattern]);
    }
    command.arg("-C").arg(root).args(&present);
    local::run(TAR, command, PACK_TIMEOUT)
}
