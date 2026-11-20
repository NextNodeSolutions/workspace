use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::cli::{self, Failure};
use crate::error::BoatError;
use crate::notice::{Notice, Refusal, Warnings};
use crate::process;

const NEW: &str = "new";
const ERROR_EVENT: &str = "error";
const MAX_LINE_BYTES: u64 = 64 * 1024;
const QUEUED_LINES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
}

enum Output {
    Line(Stream, Vec<u8>),
    Closed(Stream, io::Result<()>),
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "event")]
    kind: String,
    #[serde(flatten)]
    refusal: Refusal,
}

struct Created {
    id: String,
    refusal: Option<Refusal>,
}

pub(crate) fn create(
    args: &[&str],
    warnings: &Arc<dyn Warnings>,
    finish_timeout: Duration,
) -> Result<String, BoatError> {
    let mut child = cli::spawn(NEW, args, Stdio::null())?;
    let (sender, output) = mpsc::sync_channel(QUEUED_LINES);
    read_pipe(
        child.stdout.take().expect("stdout is piped"),
        Stream::Stdout,
        sender.clone(),
    );
    read_pipe(
        child.stderr.take().expect("stderr is piped"),
        Stream::Stderr,
        sender,
    );
    let created = match read_id(&output, cli::CALL_TIMEOUT) {
        Ok(created) => created,
        Err(error) => {
            process::kill(&mut child);
            return Err(error);
        }
    };
    let id = created.id.clone();
    let warnings = Arc::clone(warnings);
    thread::spawn(move || watch(created, &output, child, &*warnings, finish_timeout));
    Ok(id)
}

fn read_pipe(pipe: impl Read + Send + 'static, stream: Stream, sender: SyncSender<Output>) {
    thread::spawn(move || {
        let result = forward_lines(BufReader::new(pipe), stream, &sender);
        let _ = sender.send(Output::Closed(stream, result));
    });
}

fn forward_lines(
    mut pipe: impl BufRead,
    stream: Stream,
    sender: &SyncSender<Output>,
) -> io::Result<()> {
    loop {
        let mut bytes = Vec::new();
        let count = (&mut pipe)
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        if count as u64 == MAX_LINE_BYTES && !bytes.ends_with(b"\n") {
            pipe.skip_until(b'\n')?;
            bytes.clear();
        }
        if sender.send(Output::Line(stream, bytes)).is_err() {
            return Ok(());
        }
    }
}

fn read_id(output: &Receiver<Output>, timeout: Duration) -> Result<Created, BoatError> {
    let deadline = Instant::now() + timeout;
    let mut last_stdout = Vec::new();
    let mut refusal = None;
    loop {
        match receive(output, deadline) {
            Ok(Output::Line(Stream::Stdout, line)) => {
                if let Ok(Id { id }) = serde_json::from_slice(&line) {
                    return Ok(Created { id, refusal });
                }
                last_stdout = line;
            }
            Ok(Output::Line(Stream::Stderr, line)) => {
                refusal = refusal.or_else(|| refusal_in(&line));
            }
            Ok(Output::Closed(Stream::Stderr, _)) => {}
            Ok(Output::Closed(Stream::Stdout, result)) => {
                result.map_err(BoatError::pipe(NEW))?;
                return Err(pre_id_failure(&last_stdout));
            }
            Err(RecvTimeoutError::Disconnected) => return Err(pre_id_failure(&last_stdout)),
            Err(RecvTimeoutError::Timeout) => {
                return Err(BoatError::CreateStalled {
                    seconds: timeout.as_secs(),
                });
            }
        }
    }
}

fn pre_id_failure(line: &[u8]) -> BoatError {
    Failure::last_in(line).map_or(BoatError::NoId, |failure| failure.into_error(NEW))
}

fn watch(
    created: Created,
    output: &Receiver<Output>,
    mut child: Child,
    warnings: &dyn Warnings,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    let mut reported = report_refusal(warnings, &created.id, created.refusal);
    let mut lost_output = false;
    let did_drain = loop {
        match receive(output, deadline) {
            Ok(Output::Line(_, line)) if !reported => {
                reported = report_refusal(warnings, &created.id, refusal_in(&line));
            }
            Ok(Output::Closed(_, result)) => lost_output |= result.is_err(),
            Ok(Output::Line(..)) => {}
            Err(RecvTimeoutError::Disconnected) => break true,
            Err(RecvTimeoutError::Timeout) => break false,
        }
    };
    let result = process::wait_until(NEW, &mut child, deadline);
    if !reported {
        report_exit(warnings, &created.id, &result, did_drain && !lost_output);
    }
}

fn receive(output: &Receiver<Output>, deadline: Instant) -> Result<Output, RecvTimeoutError> {
    match output.try_recv() {
        Ok(item) => return Ok(item),
        Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
        Err(TryRecvError::Empty) => {}
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(RecvTimeoutError::Timeout);
    }
    output.recv_timeout(remaining)
}

fn refusal_in(line: &[u8]) -> Option<Refusal> {
    let event: Event = serde_json::from_slice(line).ok()?;
    (event.kind == ERROR_EVENT).then_some(event.refusal)
}

fn report_refusal(warnings: &dyn Warnings, id: &str, refusal: Option<Refusal>) -> bool {
    if let Some(refusal) = refusal {
        warnings.warning(Notice::refused(NEW, id, refusal));
        return true;
    }
    false
}

fn report_exit(
    warnings: &dyn Warnings,
    id: &str,
    result: &Result<Option<std::process::ExitStatus>, BoatError>,
    has_complete_output: bool,
) {
    let notice = match result {
        Ok(Some(status)) if !status.success() => Notice::ended(NEW, id, *status),
        Ok(Some(_)) if has_complete_output => return,
        Ok(_) | Err(_) => Notice::unconfirmed(NEW, id),
    };
    warnings.warning(notice);
}
