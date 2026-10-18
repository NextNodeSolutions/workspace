//! Runs the `boat` CLI. Every call asks for JSON and skips the update check.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::BoatError;
use crate::process;

const PROGRAM: &str = "boat";
const FLAGS: [&str; 2] = ["--json", "--no-update"];

/// Bounds the calls that only talk to the Boat API. They answer within
/// seconds, so a call past this has stalled.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// `boat <subcommand>` with the flags every call takes, then `args`.
pub(crate) fn command(subcommand: &'static str, args: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command.arg(subcommand).args(FLAGS).args(args);
    command
}

/// Starts `boat <subcommand> <args>` with its stdout and stderr piped.
pub(crate) fn spawn(
    subcommand: &'static str,
    args: &[&str],
    stdin: Stdio,
) -> Result<Child, BoatError> {
    command(subcommand, args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(BoatError::Spawn)
}

/// Runs `boat <subcommand> <args>` to the end, within [`CALL_TIMEOUT`], and
/// reads the JSON it prints.
pub(crate) fn call<T: DeserializeOwned>(
    subcommand: &'static str,
    args: &[&str],
) -> Result<T, BoatError> {
    let child = spawn(subcommand, args, Stdio::null())?;
    let output = process::collect(subcommand, child, &[], Some(CALL_TIMEOUT))?;
    if !output.status.success() {
        let failure = Failure::last_in(&output.stdout)
            .unwrap_or_else(|| Failure::from_stderr(&output.stderr));
        return Err(failure.into_error(subcommand));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|source| BoatError::Unreadable { subcommand, source })
}

/// The JSON line `boat` ends its output with when it fails.
#[derive(Deserialize)]
pub(crate) struct Failure {
    #[serde(rename = "error")]
    message: String,
    /// Set when the Boat API refused the request.
    code: Option<String>,
}

impl Failure {
    /// The failure the last line of `stdout` reports, if it reports one.
    pub(crate) fn last_in(stdout: &[u8]) -> Option<Self> {
        let stdout = String::from_utf8_lossy(stdout);
        let last_line = stdout.lines().last()?;
        serde_json::from_str(last_line).ok()
    }

    /// Whether the Boat API refused the request, rather than the CLI or the
    /// command it ran failing.
    pub(crate) fn is_refusal(&self) -> bool {
        self.code.is_some()
    }

    pub(crate) fn into_error(self, subcommand: &'static str) -> BoatError {
        BoatError::Refused {
            subcommand,
            code: self.code,
            message: self.message,
        }
    }

    fn from_stderr(stderr: &[u8]) -> Self {
        Self {
            message: String::from_utf8_lossy(stderr).trim().to_owned(),
            code: None,
        }
    }
}
