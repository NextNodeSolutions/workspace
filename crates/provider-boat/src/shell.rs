use std::process::{Output, Stdio};

use workspace_core::port::Shell;
use workspace_core::{Cause, VmId};

use crate::cli::{self, Failure};
use crate::error::BoatError;
use crate::{process, sandbox};

const SSH: &str = "ssh";

pub(crate) struct BoatShell {
    vm: VmId,
}

impl BoatShell {
    pub(crate) fn new(vm: VmId) -> Self {
        Self { vm }
    }

    fn execute(&self, script: &str, stdin: &[u8]) -> Result<Output, BoatError> {
        let args = [self.vm.as_str(), "--", "bash", "-c", script];
        let child = cli::spawn(SSH, &args, Stdio::piped())?;
        process::collect(SSH, child, stdin, None)
    }
}

/// Runs `boat ssh <id>` on the user's terminal. There is no timeout: the
/// session lasts as long as the person wants. Its exit status is that of the
/// last command typed in it, and Boat's own refusals exit the same way, so
/// after a failure the state decides: a sandbox that no longer runs had no
/// session to give.
pub(crate) fn attach(vm: &VmId) -> Result<(), BoatError> {
    let status = cli::interactive(SSH, &[vm.as_str()])
        .status()
        .map_err(BoatError::Spawn)?;
    if status.success() {
        return Ok(());
    }
    let state = sandbox::read(vm.as_str())?;
    if state.is_running() {
        return Ok(());
    }
    Err(BoatError::NotRunning { state })
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

fn failure(output: &Output) -> BoatError {
    let stderr = String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_owned();
    let Some(failure) = Failure::last_in(&output.stdout) else {
        let reason = format!("`boat ssh` ended with {}", output.status);
        return BoatError::Script { reason, stderr };
    };
    match failure.into_error(SSH) {
        BoatError::Refused {
            code: None,
            message,
            ..
        } => BoatError::Script {
            reason: message,
            stderr,
        },
        refusal => refusal,
    }
}
