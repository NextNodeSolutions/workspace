//! Owns a `boat` child process: feeds its stdin, collects what it prints,
//! and kills it once its timeout passes.

use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, ExitStatus, Output};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::BoatError;

const BYTES_PER_MIB: u64 = 1024 * 1024;

/// Past this size, the rest of a stream is read and dropped, so a command
/// that prints without end cannot fill the local memory.
pub(crate) const MAX_OUTPUT_MIB: u64 = 64;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// What one stream printed, up to [`MAX_OUTPUT_MIB`].
struct Captured {
    bytes: Vec<u8>,
    is_truncated: bool,
}

/// Feeds `stdin` to `child`, waits for it to exit and returns what it
/// printed. With a `timeout`, kills it once the timeout passes.
pub(crate) fn collect(
    subcommand: &'static str,
    mut child: Child,
    stdin: &[u8],
    timeout: Option<Duration>,
) -> Result<Output, BoatError> {
    if let Some(input) = child.stdin.take() {
        let bytes = stdin.to_vec();
        thread::spawn(move || feed(input, &bytes));
    }
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout = thread::spawn(move || capture(stdout));
    let stderr = thread::spawn(move || capture(stderr));
    // After a kill, the readers are left behind: a pipe that a leftover
    // process still holds open must not delay the error.
    let status = supervise(subcommand, &mut child, timeout)?;
    let stdout = join(stdout).map_err(BoatError::pipe(subcommand))?;
    let stderr = join(stderr).map_err(BoatError::pipe(subcommand))?;
    if stdout.is_truncated || stderr.is_truncated {
        return Err(BoatError::TooMuchOutput {
            subcommand,
            limit_mib: MAX_OUTPUT_MIB,
        });
    }
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

/// Kills `child` and reaps it. Both outcomes are dropped: the kill fails only
/// when the child already exited, and the wait then reaps it all the same.
pub(crate) fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn supervise(
    subcommand: &'static str,
    child: &mut Child,
    timeout: Option<Duration>,
) -> Result<ExitStatus, BoatError> {
    let Some(timeout) = timeout else {
        return child.wait().map_err(BoatError::pipe(subcommand));
    };
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(BoatError::pipe(subcommand))? {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    kill(child);
    Err(BoatError::Stalled {
        subcommand,
        seconds: timeout.as_secs(),
    })
}

/// A command may exit before reading all of its stdin. The broken pipe that
/// leaves is not a failure, so the outcome of the write is dropped.
fn feed(mut input: ChildStdin, bytes: &[u8]) {
    let _ = input.write_all(bytes);
}

/// Keeps reading past the limit, so the command never blocks on a full pipe.
fn capture(mut pipe: impl Read) -> io::Result<Captured> {
    let mut bytes = Vec::new();
    pipe.by_ref()
        .take(MAX_OUTPUT_MIB * BYTES_PER_MIB)
        .read_to_end(&mut bytes)?;
    let dropped = io::copy(&mut pipe, &mut io::sink())?;
    Ok(Captured {
        bytes,
        is_truncated: dropped > 0,
    })
}

fn join<T>(handle: JoinHandle<T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}
