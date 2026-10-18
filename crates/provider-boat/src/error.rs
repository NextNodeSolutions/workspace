use std::io;
use std::string::FromUtf8Error;

use crate::sandbox::State;

/// The code Boat answers with for a sandbox that does not exist, or no
/// longer does.
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

    #[error("`boat new` ended without printing a sandbox id")]
    NoId,

    #[error("the sandbox is {state}, so it cannot be reached")]
    NotRunning { state: State },

    #[error("the sandbox is {state}, so the stop did not finish")]
    NotStopped { state: State },

    #[error("the sandbox is still {state} after {seconds} s")]
    TimedOut { state: State, seconds: u64 },

    #[error("the script failed on the sandbox:\n{stderr}")]
    Script { stderr: String },

    #[error("the script printed output that is not UTF-8")]
    NotUtf8(#[source] FromUtf8Error),
}

impl BoatError {
    /// Whether Boat answered that the sandbox does not exist.
    pub(crate) fn is_gone(&self) -> bool {
        matches!(self, Self::Refused { code: Some(code), .. } if code == NOT_FOUND)
    }
}
