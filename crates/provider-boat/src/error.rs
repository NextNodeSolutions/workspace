use std::io;
use std::string::FromUtf8Error;

use workspace_core::{Failure, Reason};

use crate::options::{BoatName, InvalidOption, MachineType};
use crate::sandbox::State;

const NOT_FOUND: &str = "not_found";

#[derive(Debug, thiserror::Error)]
pub(crate) enum BoatError {
    #[error("cannot run `boat`: install the Boat CLI and put it on PATH")]
    Spawn(#[source] io::Error),

    #[error("lost the pipe to `boat {subcommand}`")]
    Pipe {
        subcommand: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("`boat {subcommand}` failed: {message}")]
    Refused {
        subcommand: &'static str,
        code: Option<String>,
        message: String,
    },

    #[error("`boat {subcommand}` printed JSON this adapter cannot read")]
    Unreadable {
        subcommand: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "`boat {subcommand}` did not finish within {seconds} s, so it was killed, though Boat may still carry it out"
    )]
    Stalled {
        subcommand: &'static str,
        seconds: u64,
    },

    #[error("`boat {subcommand}` printed more than {limit_mib} MiB on one stream")]
    TooMuchOutput {
        subcommand: &'static str,
        limit_mib: u64,
    },

    #[error("the Boat CLI is not signed in: run `boat login`")]
    SignedOut,

    #[error("this Boat account cannot start `{machine_type}` sandboxes: {reason}")]
    Unavailable {
        machine_type: MachineType,
        reason: String,
    },

    #[error("the Boat environment `{name}` does not exist")]
    NoEnvironment { name: BoatName },

    #[error(
        "`{name}` is the default Boat environment, which other sandboxes start from, so setup leaves it alone: name another one with --boat-environment or `environment` in the config file"
    )]
    DefaultEnvironment { name: BoatName },

    #[error("the Boat environment `{name}` no longer matches what setup sets: {changed}")]
    Mismatch { name: BoatName, changed: String },

    #[error("`boat new` ended without printing a sandbox id")]
    NoId,

    #[error(
        "`boat new` printed no sandbox id within {seconds} s, so it was killed; `boat list` shows whether it created one"
    )]
    CreateStalled { seconds: u64 },

    #[error("the sandbox is {state}, so it cannot be reached")]
    NotRunning { state: State },

    #[error("the sandbox is {state}, so it cannot resume")]
    NotResumable { state: State },

    #[error("Boat no longer has this sandbox, its disk included")]
    Gone,

    #[error("the sandbox is {state}, so the stop did not finish")]
    NotStopped { state: State },

    #[error("the sandbox is still {state} after {seconds} s")]
    TimedOut { state: State, seconds: u64 },

    #[error("the script failed on the sandbox: {reason}{}", on_new_line(.stderr))]
    Script { reason: String, stderr: String },

    #[error("the script printed output that is not UTF-8")]
    NotUtf8(#[source] FromUtf8Error),

    #[error(transparent)]
    Option(#[from] InvalidOption),
}

impl BoatError {
    pub(crate) fn pipe(subcommand: &'static str) -> impl Fn(io::Error) -> Self {
        move |source| Self::Pipe { subcommand, source }
    }

    pub(crate) fn is_refusal(&self) -> bool {
        matches!(self, Self::Refused { code: Some(_), .. })
    }

    pub(crate) fn is_gone(&self) -> bool {
        matches!(self, Self::Refused { code: Some(code), .. } if code == NOT_FOUND)
    }

    fn reason(&self) -> Reason {
        match self {
            Self::NoEnvironment { .. } | Self::Mismatch { .. } => Reason::NotSetUp,
            Self::Gone | Self::NotRunning { state: State::Gone } => Reason::Gone,
            error if error.is_gone() => Reason::Gone,
            _ => Reason::Other,
        }
    }
}

impl From<BoatError> for Failure {
    fn from(error: BoatError) -> Self {
        Failure::new(error.reason(), error)
    }
}

fn on_new_line(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!("\n{text}")
}
