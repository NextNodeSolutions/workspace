use std::path::{Path, PathBuf};
use std::{fs, io};

use workspace_core::port::Ledger;
use workspace_core::{Cause, VmId, VmSpec};

use crate::xdg;

const RECORDS_DIRECTORY: &str = "workspace/vms";
const DEFAULT_DATA_HOME: &str = ".local/share";
const PROVIDER_FIELD: &str = "provider=";
const NAME_FIELD: &str = "name=";

/// Starts the name of a file that is not a record, such as one being
/// written. A record never starts with it, since its id cannot.
const HIDDEN_PREFIX: char = '.';

pub struct FileLedger {
    directory: PathBuf,
    provider: String,
}

/// A VM as its record describes it.
pub struct Record {
    pub id: VmId,
    pub provider: String,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("cannot locate the data directory: set XDG_DATA_HOME or HOME")]
    NoDataDirectory,
    #[error("vm id `{0}` cannot name a record file")]
    UnsafeId(VmId),
    #[error("no VM is recorded as `{0}`: `workspace list` shows the recorded VMs")]
    Unknown(String),
    #[error(
        "several VMs are named `{name}`: {ids}. `stop` and `remove` take an id, so remove all but one"
    )]
    Ambiguous { name: String, ids: String },
    #[error("{} has no `{field}` line", path.display())]
    Incomplete { path: PathBuf, field: &'static str },
    #[error("cannot {action} {}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl FileLedger {
    pub fn for_provider(provider: &str) -> Result<Self, LedgerError> {
        Ok(Self {
            directory: records_directory()?,
            provider: provider.to_owned(),
        })
    }
}

impl Ledger for FileLedger {
    fn record(&self, vm: &VmId, spec: &VmSpec) -> Result<(), Cause> {
        let path = record_path(&self.directory, vm)?;
        // A leading dot never collides with a record: record names cannot start with one.
        let staging = self.directory.join(format!("{HIDDEN_PREFIX}{vm}.tmp"));
        let record = format!(
            "{PROVIDER_FIELD}{}\n{NAME_FIELD}{}\n",
            self.provider,
            spec.name()
        );
        fs::create_dir_all(&self.directory).map_err(io_error("create", &self.directory))?;
        fs::write(&staging, record).map_err(io_error("write", &staging))?;
        fs::rename(&staging, &path).map_err(io_error("write", &path))?;
        Ok(())
    }

    fn forget(&self, vm: &VmId) -> Result<(), Cause> {
        let path = record_path(&self.directory, vm)?;
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(io_error("remove", &path)(error).into())
            }
            _ => Ok(()),
        }
    }
}

/// Every record, sorted by name, then by id.
pub fn records() -> Result<Vec<Record>, LedgerError> {
    let directory = records_directory()?;
    let entries = match fs::read_dir(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        entries => entries.map_err(io_error("read", &directory))?,
    };
    let mut records = Vec::new();
    for entry in entries {
        let file_name = entry.map_err(io_error("read", &directory))?.file_name();
        let id = file_name.to_string_lossy();
        if !id.starts_with(HIDDEN_PREFIX) {
            records.extend(read_record(&directory, &VmId::from(id.into_owned()))?);
        }
    }
    records.sort_by(|left, right| {
        (&left.name, left.id.as_str()).cmp(&(&right.name, right.id.as_str()))
    });
    Ok(records)
}

/// The record of the VM that `handle` names: its id, or else its name.
pub fn find(handle: &str) -> Result<Record, LedgerError> {
    let directory = records_directory()?;
    if let Some(record) = read_record(&directory, &VmId::from(handle.to_owned()))? {
        return Ok(record);
    }
    named(handle)?.ok_or_else(|| LedgerError::Unknown(handle.to_owned()))
}

/// The record of the VM named `name`, when one is recorded.
pub fn named(name: &str) -> Result<Option<Record>, LedgerError> {
    let mut matches: Vec<Record> = records()?
        .into_iter()
        .filter(|record| record.name == name)
        .collect();
    if matches.len() > 1 {
        let ids: Vec<&str> = matches.iter().map(|record| record.id.as_str()).collect();
        return Err(LedgerError::Ambiguous {
            name: name.to_owned(),
            ids: ids.join(", "),
        });
    }
    Ok(matches.pop())
}

fn records_directory() -> Result<PathBuf, LedgerError> {
    let data_home = xdg::base_directory("XDG_DATA_HOME", DEFAULT_DATA_HOME)
        .ok_or(LedgerError::NoDataDirectory)?;
    Ok(data_home.join(RECORDS_DIRECTORY))
}

fn record_path(directory: &Path, vm: &VmId) -> Result<PathBuf, LedgerError> {
    let id = vm.as_str();
    let is_file_name =
        !id.is_empty() && !id.starts_with(HIDDEN_PREFIX) && !id.contains(['/', '\\', '\0']);
    if !is_file_name {
        return Err(LedgerError::UnsafeId(vm.clone()));
    }
    Ok(directory.join(id))
}

/// The record of `vm`, or `None` when it has none.
fn read_record(directory: &Path, vm: &VmId) -> Result<Option<Record>, LedgerError> {
    let path = record_path(directory, vm)?;
    let text = match fs::read_to_string(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        text => text.map_err(io_error("read", &path))?,
    };
    let field = |field: &'static str| {
        let value = text.lines().find_map(|line| line.strip_prefix(field));
        value
            .map(str::to_owned)
            .ok_or_else(|| LedgerError::Incomplete {
                path: path.clone(),
                field,
            })
    };
    Ok(Some(Record {
        id: vm.clone(),
        provider: field(PROVIDER_FIELD)?,
        name: field(NAME_FIELD)?,
    }))
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> LedgerError {
    let path = path.to_owned();
    move |source| LedgerError::Io {
        action,
        path,
        source,
    }
}
