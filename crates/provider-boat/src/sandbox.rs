use std::fmt;
use std::io::{self, BufRead, BufReader};
use std::process::{ChildStdout, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::cli::{self, Failure};
use crate::error::BoatError;
use crate::process;

const NEW: &str = "new";
const RESUME: &str = "resume";

/// Without it, `boat resume` waits until the sandbox is ready. `shell` does
/// that wait instead, within its own time limit, as after `boat new`.
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
    /// Whether the sandbox runs, or is on its way to running.
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Up | Self::Starting)
    }

    pub(crate) fn is_starting(&self) -> bool {
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

#[derive(Deserialize)]
struct Event {
    id: Option<String>,
}

enum Created {
    Id(String),
    Ended { last_line: String },
}

pub(crate) fn create(args: &[&str]) -> Result<String, BoatError> {
    let mut child = cli::command(NEW, args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(BoatError::Spawn)?;
    let lines = forward_lines(child.stdout.take().expect("stdout is piped"));
    match read_id(&lines, cli::CALL_TIMEOUT) {
        Ok(Created::Id(id)) => {
            thread::spawn(move || child.wait());
            Ok(id)
        }
        Ok(Created::Ended { last_line }) => {
            child.wait().map_err(BoatError::pipe(NEW))?;
            let failure = Failure::last_in(last_line.as_bytes());
            Err(failure.map_or(BoatError::NoId, |failure| failure.into_error(NEW)))
        }
        Err(error) => {
            process::kill(&mut child);
            Err(error)
        }
    }
}

fn forward_lines(stdout: ChildStdout) -> Receiver<io::Result<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let _ = sender.send(line);
        }
    });
    receiver
}

fn read_id(lines: &Receiver<io::Result<String>>, timeout: Duration) -> Result<Created, BoatError> {
    let deadline = Instant::now() + timeout;
    let mut last_line = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = match lines.recv_timeout(remaining) {
            Ok(line) => line.map_err(BoatError::pipe(NEW))?,
            Err(RecvTimeoutError::Disconnected) => return Ok(Created::Ended { last_line }),
            Err(RecvTimeoutError::Timeout) => {
                return Err(BoatError::CreateStalled {
                    seconds: timeout.as_secs(),
                });
            }
        };
        if let Some(id) = sandbox_id(&line) {
            return Ok(Created::Id(id));
        }
        last_line = line;
    }
}

fn sandbox_id(line: &str) -> Option<String> {
    let event: Event = serde_json::from_str(line).ok()?;
    event.id
}

pub(crate) fn resume(id: &str) -> Result<(), BoatError> {
    let refusal = match cli::run(RESUME, &[id, NO_WAIT]) {
        Err(error) if error.is_refusal() => error,
        outcome => return outcome.map(drop),
    };
    if read(id)?.is_running() {
        return Ok(());
    }
    Err(refusal)
}

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
