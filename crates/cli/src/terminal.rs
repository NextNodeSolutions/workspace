use std::fmt;
use std::process::Command;
use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use workspace_core::Cause;
use workspace_core::duration;
use workspace_core::port::{SessionEnd, Terminal};

use crate::relay;

const OFF: &str = "off";

const DEFAULT_IDLE: Duration = Duration::from_secs(5 * 60);

/// How long a session may pass nothing through the terminal, in either
/// direction, before the VM is stopped; `off` for never.
#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Idle(Option<Duration>);

impl Idle {
    #[must_use]
    pub fn or_default(idle: Option<Self>) -> Self {
        idle.unwrap_or(Self(Some(DEFAULT_IDLE)))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not an idle duration: use a count and a unit, such as 5m, 90s or 2h, or `off`")]
pub struct InvalidIdle(String);

impl FromStr for Idle {
    type Err = InvalidIdle;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == OFF {
            return Ok(Self(None));
        }
        duration::parse(text)
            .map(|duration| Self(Some(duration)))
            .ok_or_else(|| InvalidIdle(text.to_owned()))
    }
}

impl TryFrom<String> for Idle {
    type Error = InvalidIdle;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for Idle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(duration) => formatter.write_str(&duration::show(duration)),
            None => formatter.write_str(OFF),
        }
    }
}

/// This machine's terminal: the program runs in a pseudo-terminal, your
/// keystrokes go to it and its output comes back, and both count as activity.
pub struct LocalTerminal {
    idle: Idle,
}

impl LocalTerminal {
    #[must_use]
    pub fn new(idle: Idle) -> Self {
        Self { idle }
    }
}

impl Terminal for LocalTerminal {
    fn run(&self, program: Command) -> Result<SessionEnd, Cause> {
        Ok(relay::run(&program, self.idle.0)?)
    }
}
