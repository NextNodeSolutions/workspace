use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, ExitStatus, Output};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::BoatError;

const BYTES_PER_MIB: u64 = 1024 * 1024;

pub(crate) const MAX_OUTPUT_MIB: u64 = 64;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

struct Captured {
    bytes: Vec<u8>,
    is_truncated: bool,
}

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

/// Both outcomes are dropped: a kill fails only on an exited child, which the wait reaps anyway.
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

/// A command may exit before reading all its stdin, so a broken pipe here is not a failure.
fn feed(mut input: ChildStdin, bytes: &[u8]) {
    let _ = input.write_all(bytes);
}

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
