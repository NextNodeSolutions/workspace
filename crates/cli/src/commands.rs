use std::error::Error;
use std::path::Path;

use workspace_core::lifecycle;

use crate::ProviderArgs;
use crate::adapters;
use crate::config::{self, Config, Providers};
use crate::ledger::{self, FileLedger, Record, Records};

const COLUMN_GAP: &str = "  ";

const DISPOSABLE_MARK: &str = "-";

#[derive(Debug, thiserror::Error)]
#[error(
    "the records above cannot be read, so their VMs may run unlisted: fix or delete each file once its VM is gone"
)]
struct UnreadableRecords;

pub fn setup(
    args: ProviderArgs,
    file: Config,
    config_path: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let ProviderArgs { provider, options } = args;
    let name = match provider.or(file.provider) {
        Some(name) => name,
        None => adapters::sole_provider()?,
    };
    let provider = adapters::provider(&name, options.or(file.providers))?;
    let summary = lifecycle::setup(provider.as_ref())?;
    // Printed first, so a config file that cannot be written still leaves on
    // screen what changed.
    println!("{summary}");
    let path = config::remember_provider(config_path, &name)?;
    println!(
        "Commands run on {name} unless --provider names another, as {} sets.",
        path.display()
    );
    Ok(())
}

/// A disposable VM has no disk to keep, so stopping it deletes it.
pub fn stop(vm: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    if record.name.is_none() {
        let ledger = FileLedger::for_provider(&record.provider)?;
        lifecycle::remove(provider.as_ref(), &ledger, &record.id)?;
        eprintln!(
            "{} was disposable, so it is deleted rather than stopped.",
            record.id
        );
        return Ok(());
    }
    lifecycle::stop(provider.as_ref(), &record.id)?;
    Ok(())
}

pub fn snapshot(vm: &str, name: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    lifecycle::snapshot(provider.as_ref(), &record.id, name)?;
    println!("{name}");
    eprintln!(
        "New VMs start from it once the provider option that names a snapshot is set to `{name}`."
    );
    Ok(())
}

pub fn remove(vm: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    let ledger = FileLedger::for_provider(&record.provider)?;
    lifecycle::remove(provider.as_ref(), &ledger, &record.id)?;
    Ok(())
}

pub fn list() -> Result<(), Box<dyn Error>> {
    let Records { found, unreadable } = ledger::records()?;
    let name_width = found.iter().map(|record| shown_name(record).len()).max();
    let id_width = found.iter().map(|record| record.id.as_str().len()).max();
    let (name_width, id_width) = (name_width.unwrap_or(0), id_width.unwrap_or(0));
    for record in &found {
        let (name, id) = (shown_name(record), record.id.as_str());
        println!(
            "{name:name_width$}{COLUMN_GAP}{id:id_width$}{COLUMN_GAP}{}",
            record.provider
        );
    }
    for error in &unreadable {
        crate::report(error);
    }
    if unreadable.is_empty() {
        Ok(())
    } else {
        Err(UnreadableRecords.into())
    }
}

fn shown_name(record: &Record) -> &str {
    record.name.as_deref().unwrap_or(DISPOSABLE_MARK)
}
