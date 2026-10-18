//! Creates a sandbox, reads its state, and waits for that state to change.

use std::fmt;
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::cli::{self, Failure};
use crate::error::BoatError;

const NEW: &str = "new";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Where a sandbox stands, as far as this adapter cares.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Accepts ssh.
    Up,
    Starting,
    /// Boat snapshots the disk, then stops the machine.
    Stopping,
    /// Nothing runs, and the disk is kept.
    Stopped,
    /// Deleted, or never existed.
    Gone,
    /// Any other state, such as `error`, as Boat names it.
    Other(String),
}

impl State {
    pub(crate) fn is_starting(&self) -> bool {
        *self == Self::Starting
    }

    pub(crate) fn is_stopping(&self) -> bool {
        *self == Self::Stopping
    }

    /// The CLI shows some API states under another name, `provisioned` as
    /// `starting` and `archived` as `stopped`, so both names are accepted.
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

/// What `boat info` prints, down to the one field read here.
#[derive(Deserialize)]
struct Info {
    sandbox: Sandbox,
}

#[derive(Deserialize)]
struct Sandbox {
    state: String,
}

/// A line `boat new` prints. Its first line already carries the id.
#[derive(Deserialize)]
struct Event {
    id: Option<String>,
}

/// Starts `boat new <args>` and returns the sandbox id as soon as a line
/// carries it. The CLI then runs on in the background until it has printed
/// the rest, which nobody reads.
pub(crate) fn create(args: &[&str]) -> Result<String, BoatError> {
    let mut child = cli::command(NEW, args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(BoatError::Spawn)?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut lines = BufReader::new(stdout).lines();
    let mut last_line = String::new();
    while let Some(line) = lines.next() {
        let line = line.map_err(|source| BoatError::Pipe {
            subcommand: NEW,
            source,
        })?;
        if let Some(id) = sandbox_id(&line) {
            thread::spawn(move || {
                lines.for_each(drop);
                child.wait()
            });
            return Ok(id);
        }
        last_line = line;
    }
    child.wait().map_err(|source| BoatError::Pipe {
        subcommand: NEW,
        source,
    })?;
    let failure = Failure::last_in(last_line.as_bytes());
    Err(failure.map_or(BoatError::NoId, |failure| failure.into_error(NEW)))
}

fn sandbox_id(line: &str) -> Option<String> {
    let event: Event = serde_json::from_str(line).ok()?;
    event.id
}

/// Reads the state of sandbox `id` every [`POLL_INTERVAL`] while
/// `is_transient` holds, and returns the first state for which it does not.
pub(crate) fn wait_while(
    id: &str,
    timeout: Duration,
    is_transient: fn(&State) -> bool,
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

fn read(id: &str) -> Result<State, BoatError> {
    match cli::call::<Info>("info", &[id]) {
        Ok(info) => Ok(State::from_boat(info.sandbox.state)),
        Err(error) if error.is_gone() => Ok(State::Gone),
        Err(error) => Err(error),
    }
}
