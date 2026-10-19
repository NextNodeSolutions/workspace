use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::BoatError;
use crate::process;

const PROGRAM: &str = "boat";
const FLAGS: [&str; 2] = ["--json", "--no-update"];

/// Boat API calls answer within seconds, so one past this has stalled.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn command(subcommand: &'static str, args: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command.args(subcommand.split(' ')).args(FLAGS).args(args);
    command
}

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

pub(crate) fn call<T: DeserializeOwned>(
    subcommand: &'static str,
    args: &[&str],
) -> Result<T, BoatError> {
    let stdout = run(subcommand, args)?;
    serde_json::from_slice(&stdout).map_err(|source| BoatError::Unreadable { subcommand, source })
}

pub(crate) fn run(subcommand: &'static str, args: &[&str]) -> Result<Vec<u8>, BoatError> {
    let child = spawn(subcommand, args, Stdio::null())?;
    let output = process::collect(subcommand, child, &[], Some(CALL_TIMEOUT))?;
    if !output.status.success() {
        let failure = Failure::last_in(&output.stdout)
            .unwrap_or_else(|| Failure::from_stderr(&output.stderr));
        return Err(failure.into_error(subcommand));
    }
    Ok(output.stdout)
}

#[derive(Deserialize)]
pub(crate) struct Failure {
    #[serde(rename = "error")]
    message: String,
    code: Option<String>,
}

impl Failure {
    pub(crate) fn last_in(stdout: &[u8]) -> Option<Self> {
        let stdout = String::from_utf8_lossy(stdout);
        let last_line = stdout.lines().last()?;
        serde_json::from_str(last_line).ok()
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
