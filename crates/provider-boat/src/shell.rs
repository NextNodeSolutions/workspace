//! Runs scripts on a sandbox over `boat ssh`. `boat exec` would avoid ssh,
//! but it takes no stdin, and Boat keeps the text and output of every exec
//! for 90 days: secrets could only travel in the command.

use std::process::{Output, Stdio};

use workspace_core::port::Shell;
use workspace_core::{Cause, VmId};

use crate::cli::{self, Failure};
use crate::error::BoatError;
use crate::process;

const SSH: &str = "ssh";

pub(crate) struct BoatShell {
    vm: VmId,
}

impl BoatShell {
    pub(crate) fn new(vm: VmId) -> Self {
        Self { vm }
    }

    /// `boat ssh` quotes each argument for the remote shell, so the script
    /// goes through as one argument to `bash -c`. There is no timeout: a
    /// provisioning script may rightly run for a long time.
    fn execute(&self, script: &str, stdin: &[u8]) -> Result<Output, BoatError> {
        let args = [self.vm.as_str(), "--", "bash", "-c", script];
        let child = cli::spawn(SSH, &args, Stdio::piped())?;
        process::collect(SSH, child, stdin, None)
    }
}

impl Shell for BoatShell {
    fn run(&self, script: &str, stdin: &[u8]) -> Result<String, Cause> {
        let output = self.execute(script, stdin)?;
        if !output.status.success() {
            return Err(failure(&output).into());
        }
        Ok(String::from_utf8(output.stdout).map_err(BoatError::NotUtf8)?)
    }
}

/// `boat ssh` exits 1 whatever the script's exit code, and appends a JSON
/// line to stdout. That line carries a code only when the Boat API refused,
/// for example on a stopped sandbox. Otherwise the script, or ssh, failed.
fn failure(output: &Output) -> BoatError {
    match Failure::last_in(&output.stdout) {
        Some(refusal) if refusal.is_refusal() => refusal.into_error(SSH),
        _ => BoatError::Script {
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_owned(),
        },
    }
}
