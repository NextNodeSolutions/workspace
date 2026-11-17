use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub(super) enum ProcessError {
    #[error(
        "local {0} failed while preparing --push; check the repository and installed git, tar and gzip tools"
    )]
    Failed(&'static str),
    #[error("local {0} exceeded the --push preparation time limit")]
    Timeout(&'static str),
    #[error("--push input exceeds its size limit of {0} bytes")]
    TooLarge(usize),
    #[error("could not prepare --push input")]
    Io(#[from] io::Error),
}

pub(super) fn capture(
    program: &'static str,
    mut command: Command,
    stdin: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, ProcessError> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().expect("stdin is piped");
    thread::spawn(move || {
        let _ = input.write_all(&stdin);
    });
    let output = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read(output, limit));
    });
    let outcome = collect(&mut child, &receiver, program, limit);
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let (status, bytes) = outcome?;
    if !status.success() {
        return Err(ProcessError::Failed(program));
    }
    Ok(bytes)
}

fn collect(
    child: &mut Child,
    receiver: &Receiver<io::Result<Vec<u8>>>,
    program: &'static str,
    limit: usize,
) -> Result<(ExitStatus, Vec<u8>), ProcessError> {
    let deadline = Instant::now() + CALL_TIMEOUT;
    let mut output = None;
    loop {
        if output.is_none() {
            output = receive(receiver)?;
        }
        if output.as_ref().is_some_and(|bytes| bytes.len() > limit) {
            return Err(ProcessError::TooLarge(limit));
        }
        match (child.try_wait()?, output.take()) {
            (Some(status), Some(bytes)) => return Ok((status, bytes)),
            (_, bytes) => output = bytes,
        }
        if Instant::now() >= deadline {
            return Err(ProcessError::Timeout(program));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn receive(receiver: &Receiver<io::Result<Vec<u8>>>) -> Result<Option<Vec<u8>>, ProcessError> {
    match receiver.try_recv() {
        Ok(bytes) => Ok(Some(bytes?)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => {
            Err(io::Error::other("--push output reader failed").into())
        }
    }
}

fn read(output: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    output
        .take(u64::try_from(limit).expect("input limit fits u64") + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
