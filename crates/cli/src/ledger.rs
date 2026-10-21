use std::path::{Path, PathBuf};
use std::{fs, io};

use workspace_core::port::Ledger;
use workspace_core::{Cause, VmId, VmSpec};

use crate::xdg;

const RECORDS_DIRECTORY: &str = "workspace/vms";
const DEFAULT_DATA_HOME: &str = ".local/share";
const PROVIDER_FIELD: &str = "provider=";
const NAME_FIELD: &str = "name=";

pub struct FileLedger {
    directory: PathBuf,
    provider: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("cannot locate the data directory: set XDG_DATA_HOME or HOME")]
    NoDataDirectory,
    #[error("vm id `{0}` cannot name a record file")]
    UnsafeId(VmId),
    #[error("no record of {0}: only VMs that `workspace start` recorded can be stopped or removed")]
    Unknown(VmId),
    #[error("{} names no provider", .0.display())]
    NoProvider(PathBuf),
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

    pub fn find(vm: &VmId) -> Result<Self, LedgerError> {
        let directory = records_directory()?;
        let path = record_path(&directory, vm)?;
        let record = read_record(&path, vm)?;
        let provider = record
            .lines()
            .find_map(|line| line.strip_prefix(PROVIDER_FIELD))
            .ok_or_else(|| LedgerError::NoProvider(path.clone()))?;
        Ok(Self {
            directory,
            provider: provider.to_owned(),
        })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }
}

impl Ledger for FileLedger {
    fn record(&self, vm: &VmId, spec: &VmSpec) -> Result<(), Cause> {
        let path = record_path(&self.directory, vm)?;
        // A leading dot never collides with a record: record names cannot start with one.
        let staging = self.directory.join(format!(".{vm}.tmp"));
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

fn records_directory() -> Result<PathBuf, LedgerError> {
    let data_home = xdg::base_directory("XDG_DATA_HOME", DEFAULT_DATA_HOME)
        .ok_or(LedgerError::NoDataDirectory)?;
    Ok(data_home.join(RECORDS_DIRECTORY))
}

fn record_path(directory: &Path, vm: &VmId) -> Result<PathBuf, LedgerError> {
    let id = vm.as_str();
    let is_file_name = !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\', '\0']);
    if !is_file_name {
        return Err(LedgerError::UnsafeId(vm.clone()));
    }
    Ok(directory.join(id))
}

fn read_record(path: &Path, vm: &VmId) -> Result<String, LedgerError> {
    fs::read_to_string(path).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => LedgerError::Unknown(vm.clone()),
        _ => io_error("read", path)(source),
    })
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> LedgerError {
    let path = path.to_owned();
    move |source| LedgerError::Io {
        action,
        path,
        source,
    }
}
