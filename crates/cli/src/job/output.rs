use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;

use crate::files::{self, FileError};
use crate::xdg;

use super::environment::Environment;
use super::script::{
    ARTIFACT_FAILED, COMPLETE, MAX_LOG_BYTES, MAX_PAYLOAD_BYTES, PROTOCOL, TOO_LARGE, TRUNCATED,
    TRUNCATION_MARKER,
};

const JOBS_DIRECTORY: &str = "workspace/jobs";
const PRIVATE_DIRECTORY: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;

pub(super) struct Output {
    pub(super) status: u8,
    pub(super) log: Vec<u8>,
    artifacts: Vec<u8>,
    artifact_issue: Option<&'static str>,
    is_truncated: bool,
    remote_record: String,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum OutputError {
    #[error("the job returned an incomplete or invalid collection")]
    Invalid,
    #[error("job logs and artifacts exceed the collection limit of {MAX_PAYLOAD_BYTES} bytes")]
    TooLarge,
    #[error(
        "{phase} exited {status}; collection incomplete: {problem}; full records remain at {record}"
    )]
    Incomplete {
        phase: &'static str,
        status: u8,
        problem: String,
        record: String,
    },
    #[error("cannot locate the data directory: set XDG_DATA_HOME or HOME")]
    NoDirectory,
    #[error("system clock cannot name a job result directory")]
    Clock(#[from] std::time::SystemTimeError),
    #[error("could not save {phase} result (exit {status}); full records remain at {record}")]
    Persist {
        phase: &'static str,
        status: u8,
        record: String,
        #[source]
        cause: FileError,
    },
    #[error(transparent)]
    Io(#[from] FileError),
}

impl Output {
    pub(super) fn parse(text: &str, limit: usize) -> Result<Self, OutputError> {
        let mut lines = text.lines().skip_while(|line| *line != PROTOCOL);
        if lines.next() != Some(PROTOCOL) {
            return Err(OutputError::Invalid);
        }
        let status = lines
            .next()
            .ok_or(OutputError::Invalid)?
            .parse()
            .map_err(|_| OutputError::Invalid)?;
        let mut artifact_issue = match lines.next() {
            Some(COMPLETE) => None,
            Some(ARTIFACT_FAILED) => Some(
                "could not collect a selected artifact; check its path and remote artifacts.error",
            ),
            Some(TOO_LARGE) => Some("artifacts exceed the collection limit"),
            _ => return Err(OutputError::Invalid),
        };
        let is_truncated = match lines.next() {
            Some(COMPLETE) => false,
            Some(TRUNCATED) => true,
            _ => return Err(OutputError::Invalid),
        };
        let remote_record = String::from_utf8(decode(lines.next(), MAX_LOG_BYTES)?)
            .map_err(|_| OutputError::Invalid)?;
        let log = decode(lines.next(), MAX_LOG_BYTES.min(limit))?;
        let artifacts =
            decode(lines.next(), limit.saturating_sub(log.len())).unwrap_or_else(|_| {
                artifact_issue = Some("invalid or oversized artifact payload");
                Vec::new()
            });
        if lines.next().is_some() || remote_record.is_empty() {
            return Err(OutputError::Invalid);
        }
        Ok(Self {
            status,
            log,
            artifacts,
            artifact_issue,
            is_truncated,
            remote_record,
        })
    }

    fn ensure_complete(&self, phase: &'static str) -> Result<(), OutputError> {
        let mut problems = Vec::new();
        if let Some(issue) = self.artifact_issue {
            problems.push(issue);
        }
        if self.is_truncated {
            problems.push("log truncated");
        }
        if problems.is_empty() {
            return Ok(());
        }
        Err(OutputError::Incomplete {
            phase,
            status: self.status,
            problem: problems.join("; "),
            record: self.remote_record.clone(),
        })
    }

    pub(super) fn save(
        &self,
        directory: &Path,
        phase: &'static str,
        environment: &Environment,
    ) -> Result<(), OutputError> {
        self.save_files(directory, phase, environment)
            .map_err(|cause| OutputError::Persist {
                phase,
                status: self.status,
                record: self.remote_record.clone(),
                cause,
            })?;
        self.ensure_complete(phase)
    }

    fn save_files(
        &self,
        directory: &Path,
        phase: &str,
        environment: &Environment,
    ) -> Result<(), FileError> {
        write(
            &directory.join(format!("{phase}.status")),
            self.status.to_string().as_bytes(),
        )?;
        let mut log = if self.is_truncated {
            environment.mask_truncated(&self.log)
        } else {
            environment.mask(&self.log)
        };
        if self.is_truncated {
            log.extend_from_slice(TRUNCATION_MARKER);
        }
        write(&directory.join(format!("{phase}.log")), &log)?;
        if self.artifact_issue.is_some() || self.is_truncated {
            write(
                &directory.join(format!("{phase}.remote")),
                &environment.mask(self.remote_record.as_bytes()),
            )?;
        }
        if !self.artifacts.is_empty() {
            write(&directory.join("artifacts.tar"), &self.artifacts)?;
        }
        Ok(())
    }
}

pub(super) fn directory() -> Result<PathBuf, OutputError> {
    let base = xdg::base_directory("XDG_DATA_HOME", xdg::DEFAULT_DATA_HOME)
        .ok_or(OutputError::NoDirectory)?
        .join(JOBS_DIRECTORY);
    fs::create_dir_all(&base).map_err(files::io_error("create", &base))?;
    loop {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = base.join(format!("run-{timestamp}-{}", std::process::id()));
        match DirBuilder::new().mode(PRIVATE_DIRECTORY).create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(files::io_error("create", &path)(error).into()),
        }
    }
}

fn decode(line: Option<&str>, limit: usize) -> Result<Vec<u8>, OutputError> {
    let line = line.ok_or(OutputError::Invalid)?;
    if line.len() > limit.div_ceil(3) * 4 {
        return Err(OutputError::TooLarge);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(line)
        .map_err(|_| OutputError::Invalid)?;
    if bytes.len() > limit {
        return Err(OutputError::TooLarge);
    }
    Ok(bytes)
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE)
        .open(path)
        .map_err(files::io_error("create", path))?
        .write_all(bytes)
        .map_err(files::io_error("write", path))?;
    Ok(())
}
