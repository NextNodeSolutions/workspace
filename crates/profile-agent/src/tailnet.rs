use std::process::Command;
use std::time::Duration;

use crate::error::AgentError;
use crate::local;

const LINK_COMMAND: &str = "tailnet-link";

const JOIN_KEY_COMMAND: &str = "join-key";

const JOIN_KEY_TIMEOUT: Duration = Duration::from_secs(60);

/// A key that joins one VM to the tailnet, from this machine's
/// `tailnet-link`, which owns the tag and the limits of what it mints.
pub(crate) fn join_key() -> Result<Vec<u8>, AgentError> {
    let mut command = Command::new(LINK_COMMAND);
    command.arg(JOIN_KEY_COMMAND);
    local::run(LINK_COMMAND, command, JOIN_KEY_TIMEOUT)
}
