//! Runs the `boat` CLI. Every call asks for JSON and skips the update check.

use std::process::Command;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::BoatError;

const PROGRAM: &str = "boat";
const FLAGS: [&str; 2] = ["--json", "--no-update"];

/// `boat <subcommand>` with the flags every call takes, then `args`.
pub(crate) fn command(subcommand: &'static str, args: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command.arg(subcommand).args(FLAGS).args(args);
    command
}

/// Runs `boat <subcommand> <args>` to the end and reads the JSON it prints.
pub(crate) fn call<T: DeserializeOwned>(
    subcommand: &'static str,
    args: &[&str],
) -> Result<T, BoatError> {
    let output = command(subcommand, args)
        .output()
        .map_err(BoatError::Spawn)?;
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
