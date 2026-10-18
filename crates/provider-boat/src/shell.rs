//! Runs scripts on a sandbox over `boat ssh`. `boat exec` would avoid ssh,
//! but it takes no stdin, and Boat keeps the text and output of every exec
//! for 90 days: secrets could only travel in the command.

use std::io::Write;
use std::process::{ChildStdin, Output, Stdio};
use std::thread;

use workspace_core::port::Shell;
use workspace_core::{Cause, VmId};

use crate::cli::{self, Failure};
use crate::error::BoatError;

const SSH: &str = "ssh";

pub(crate) struct BoatShell {
    vm: VmId,
}

impl BoatShell {
    pub(crate) fn new(vm: VmId) -> Self {
        Self { vm }
    }

    /// `boat ssh` quotes each argument for the remote shell, so the script
    /// goes through as one argument to `bash -c`.
    fn execute(&self, script: &str, stdin: &[u8]) -> Result<Output, BoatError> {
        let mut child = cli::command(SSH, &[self.vm.as_str(), "--", "bash", "-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(BoatError::Spawn)?;
        let input = child.stdin.take().expect("stdin is piped");
        // Another thread feeds stdin, so a script that writes a lot before
        // reading cannot block on a full stdout pipe.
        let output = thread::scope(|scope| {
            scope.spawn(move || feed(input, stdin));
            child.wait_with_output()
        });
        output.map_err(|source| BoatError::Pipe {
            subcommand: SSH,
            source,
        })
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

/// A script may exit before reading all of its stdin. The broken pipe that
/// leaves is not a failure, so the outcome of the write is dropped.
fn feed(mut input: ChildStdin, bytes: &[u8]) {
    let _ = input.write_all(bytes);
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
