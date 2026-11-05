use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::terminal;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use workspace_core::Cause;
use workspace_core::port::SessionEnd;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

const BUFFER_SIZE: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("cannot read the size of this terminal")]
    Size(#[source] io::Error),

    #[error("cannot put this terminal in raw mode")]
    RawMode(#[source] io::Error),

    #[error("cannot open a pseudo-terminal for the session")]
    Pty(#[source] Cause),

    #[error("cannot start the session's program")]
    Spawn(#[source] Cause),
}

/// Raw mode lasts exactly as long as the session, whatever ends it.
struct RawMode;

impl RawMode {
    fn enable() -> Result<Self, RelayError> {
        terminal::enable_raw_mode().map_err(RelayError::RawMode)?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

type Activity = Arc<Mutex<Instant>>;

pub(crate) fn run(program: &Command, idle: Option<Duration>) -> Result<SessionEnd, RelayError> {
    let size = current_size()?;
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| RelayError::Pty(error.into()))?;
    let mut child = pair
        .slave
        .spawn_command(builder(program))
        .map_err(|error| RelayError::Spawn(error.into()))?;
    drop(pair.slave);
    let output = pair
        .master
        .try_clone_reader()
        .map_err(|error| RelayError::Pty(error.into()))?;
    let input = pair
        .master
        .take_writer()
        .map_err(|error| RelayError::Pty(error.into()))?;
    let activity: Activity = Arc::new(Mutex::new(Instant::now()));
    let raw_mode = RawMode::enable()?;
    let forwarded = thread::spawn({
        let activity = Arc::clone(&activity);
        move || copy(output, io::stdout(), &activity)
    });
    thread::spawn({
        let activity = Arc::clone(&activity);
        move || copy(io::stdin(), input, &activity)
    });
    let end = supervise(child.as_mut(), pair.master.as_ref(), size, &activity, idle);
    let _ = forwarded.join();
    drop(raw_mode);
    Ok(end)
}

fn supervise(
    child: &mut dyn Child,
    master: &dyn MasterPty,
    mut size: PtySize,
    activity: &Activity,
    idle: Option<Duration>,
) -> SessionEnd {
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return SessionEnd::Left {
                success: status.success(),
            };
        }
        if let Ok(current) = current_size()
            && (current.rows, current.cols) != (size.rows, size.cols)
        {
            size = current;
            let _ = master.resize(size);
        }
        if let Some(idle) = idle
            && last_activity(activity).elapsed() >= idle
        {
            let _ = child.kill();
            let _ = child.wait();
            return SessionEnd::Idle;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Program, arguments, directory and environment travel as they are; the
/// pseudo-terminal replaces the standard streams.
fn builder(program: &Command) -> CommandBuilder {
    let mut builder = CommandBuilder::new(program.get_program());
    builder.args(program.get_args());
    if let Some(directory) = program.get_current_dir() {
        builder.cwd(directory);
    }
    for (name, value) in program.get_envs() {
        match value {
            Some(value) => builder.env(name, value),
            None => builder.env_remove(name),
        }
    }
    builder
}

fn current_size() -> Result<PtySize, RelayError> {
    let (cols, rows) = terminal::size().map_err(RelayError::Size)?;
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}

/// Ends when either side closes; a closed pseudo-terminal is the session's
/// end, so its error is not one.
fn copy(mut from: impl Read, mut to: impl Write, activity: &Activity) {
    let mut buffer = [0_u8; BUFFER_SIZE];
    loop {
        let Ok(count) = from.read(&mut buffer) else {
            return;
        };
        if count == 0 {
            return;
        }
        if to
            .write_all(&buffer[..count])
            .and_then(|()| to.flush())
            .is_err()
        {
            return;
        }
        touch(activity);
    }
}

fn touch(activity: &Activity) {
    if let Ok(mut last) = activity.lock() {
        *last = Instant::now();
    }
}

fn last_activity(activity: &Activity) -> Instant {
    activity
        .lock()
        .map_or_else(|poisoned| *poisoned.into_inner(), |last| *last)
}
