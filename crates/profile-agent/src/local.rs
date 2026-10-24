use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::AgentError;

const BYTES_PER_MIB: u64 = 1024 * 1024;

const MAX_OUTPUT_MIB: u64 = 512;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) fn home() -> Result<PathBuf, AgentError> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or(AgentError::NoHome)
}

pub(crate) fn run(
    program: &'static str,
    mut command: Command,
    timeout: Duration,
) -> Result<Vec<u8>, AgentError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| AgentError::Spawn { program, source })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout = thread::spawn(move || capture(stdout));
    let stderr = thread::spawn(move || capture(stderr));
    let status = supervise(program, &mut child, timeout)?;
    let pipe = |source| AgentError::Pipe { program, source };
    let (stdout, stderr) = (join(stdout).map_err(pipe)?, join(stderr).map_err(pipe)?);
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        return Err(AgentError::TooMuchOutput {
            program,
            limit_mib: MAX_OUTPUT_MIB,
        });
    };
    if !status.success() {
        return Err(AgentError::Failed {
            program,
            stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
        });
    }
    Ok(stdout)
}

fn supervise(
    program: &'static str,
    child: &mut Child,
    timeout: Duration,
) -> Result<ExitStatus, AgentError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let polled = child
            .try_wait()
            .map_err(|source| AgentError::Pipe { program, source })?;
        if let Some(status) = polled {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    // A kill fails only on an exited child, which the wait reaps anyway.
    let _ = child.kill();
    let _ = child.wait();
    Err(AgentError::Stalled {
        program,
        seconds: timeout.as_secs(),
    })
}

/// `None` means the stream passed the size limit; the rest is drained so the
/// child never blocks on a full pipe.
fn capture(mut pipe: impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    pipe.by_ref()
        .take(MAX_OUTPUT_MIB * BYTES_PER_MIB)
        .read_to_end(&mut bytes)?;
    let dropped = io::copy(&mut pipe, &mut io::sink())?;
    Ok((dropped == 0).then_some(bytes))
}

fn join<T>(handle: JoinHandle<T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}
