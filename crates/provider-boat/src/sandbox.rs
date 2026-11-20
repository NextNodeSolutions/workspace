use std::fmt;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::cli;
use crate::error::BoatError;

const RESUME: &str = "resume";

const STALE_STOP_WINDOW: Duration = Duration::from_secs(60);

const NO_WAIT: &str = "--no-wait";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum State {
    Up,
    Starting,
    Stopping,
    Stopped,
    Gone,
    Other(String),
}

impl State {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Up | Self::Starting)
    }

    fn is_stopped(&self) -> bool {
        *self == Self::Stopped
    }

    fn is_starting(&self) -> bool {
        *self == Self::Starting
    }

    pub(crate) fn is_stopping(&self) -> bool {
        *self == Self::Stopping
    }

    /// The CLI shows `provisioned` as `starting` and `archived` as `stopped`, so both names are accepted.
    fn from_boat(name: String) -> Self {
        match name.as_str() {
            "ready" | "idle" | "running" => Self::Up,
            "init" | "provisioning" | "provisioned" | "starting" | "cloning" => Self::Starting,
            "stopping" | "archiving" => Self::Stopping,
            "stopped" | "archived" => Self::Stopped,
            _ => Self::Other(name),
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Up => "running",
            Self::Starting => "starting",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Gone => "gone",
            Self::Other(name) => name,
        };
        formatter.write_str(name)
    }
}

#[derive(Deserialize)]
struct Info {
    sandbox: Sandbox,
}

#[derive(Deserialize)]
struct Sandbox {
    state: String,
}

pub(crate) fn resume(id: &str, options: &[&str]) -> Result<(), BoatError> {
    let mut args = vec![id, NO_WAIT];
    args.extend_from_slice(options);
    match cli::run(RESUME, &args) {
        Ok(_) => Ok(()),
        Err(refusal) if refusal.is_refusal() => read(id)?.is_running().then_some(()).ok_or(refusal),
        Err(error) => Err(error),
    }
}

pub(crate) fn wait_until_up(id: &str, timeout: Duration) -> Result<State, BoatError> {
    let stale_until = Instant::now() + STALE_STOP_WINDOW;
    wait_while(id, timeout, |state| {
        state.is_starting() || (state.is_stopped() && Instant::now() < stale_until)
    })
}

pub(crate) fn wait_while(
    id: &str,
    timeout: Duration,
    is_transient: impl Fn(&State) -> bool,
) -> Result<State, BoatError> {
    let deadline = Instant::now() + timeout;
    loop {
        let state = read(id)?;
        if !is_transient(&state) {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(BoatError::TimedOut {
                state,
                seconds: timeout.as_secs(),
            });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

pub(crate) fn read(id: &str) -> Result<State, BoatError> {
    match cli::call::<Info>("info", &[id]) {
        Ok(info) => Ok(State::from_boat(info.sandbox.state)),
        Err(error) if error.is_gone() => Ok(State::Gone),
        Err(error) => Err(error),
    }
}
