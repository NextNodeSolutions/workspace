use std::process::{Output, Stdio};

use workspace_core::port::{SessionEnd, Shell, Terminal};
use workspace_core::{Cause, VmId};

use crate::cli::{self, Failure};
use crate::error::BoatError;
use crate::{process, sandbox};

const SSH: &str = "ssh";

/// A command of its own needs a terminal of its own, as a shell gets by default.
const TTY: &str = "-t";

const END_OF_FLAGS: &str = "--";

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

/// A non-zero exit means either that the last command in the session failed
/// or that Boat refused, so the state decides: a sandbox that still runs had
/// a session, one that does not had none to give.
pub(crate) fn attach(
    vm: &VmId,
    command: &[String],
    terminal: &dyn Terminal,
) -> Result<SessionEnd, BoatError> {
    let mut args = vec![vm.as_str()];
    if !command.is_empty() {
        args.push(TTY);
        args.push(END_OF_FLAGS);
        args.extend(command.iter().map(String::as_str));
    }
    let end = terminal
        .run(cli::interactive(SSH, &args))
        .map_err(BoatError::Terminal)?;
    if end != (SessionEnd::Left { success: false }) {
        return Ok(end);
    }
    let state = sandbox::read(vm.as_str())?;
    if state.is_running() {
        return Ok(end);
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
