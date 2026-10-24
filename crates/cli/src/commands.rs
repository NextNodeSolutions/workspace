use std::error::Error;
use std::io::{self, IsTerminal};
use std::path::Path;

use workspace_core::port::{Ledger, Provider};
use workspace_core::{Cause, Error as CoreError, VmId, VmSpec, lifecycle};

use crate::ProviderArgs;
use crate::adapters;
use crate::config::{self, Config, Profiles, Providers};
use crate::ledger::{self, FileLedger, Record, Records};

const COLUMN_GAP: &str = "  ";

const DISPOSABLE_MARK: &str = "-";

#[derive(Debug, thiserror::Error)]
#[error("no terminal to open a shell in: pass -d to start the VM without one")]
struct NoTerminal;

#[derive(Debug, thiserror::Error)]
#[error("no provider chosen: run `workspace setup`, or pass --provider")]
struct NotSetUp;

#[derive(Debug, thiserror::Error)]
#[error(
    "{provider} is not ready to start VMs: run `workspace setup --provider {provider}`, with the same provider options as here"
)]
struct NeedsSetup {
    provider: String,
    #[source]
    cause: Cause,
}

#[derive(Debug, thiserror::Error)]
#[error("`{name}` ({vm}) no longer exists on its provider: `workspace remove {name}` forgets it")]
struct Vanished {
    name: String,
    vm: VmId,
    #[source]
    cause: Cause,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "the records above cannot be read, so their VMs may run unlisted: fix or delete each file once its VM is gone"
)]
struct UnreadableRecords;

#[derive(Debug, thiserror::Error)]
#[error("`{name}` runs on {recorded}, not {asked}: drop --provider, or pick another name")]
struct OtherProvider {
    name: String,
    recorded: String,
    asked: String,
}

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

pub fn start(
    name: Option<&str>,
    args: ProviderArgs,
    profile_options: Profiles,
    file: Config,
    profile_names: &[String],
    detach: bool,
) -> Result<(), Box<dyn Error>> {
    let has_terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    if !detach && !has_terminal {
        return Err(NoTerminal.into());
    }
    let spec = match name {
        Some(name) => VmSpec::named(name)?,
        None => VmSpec::disposable(),
    };
    let on_wait =
        || eprintln!("Another `workspace start` of that name is running: waiting for it.");
    let lock = spec
        .name()
        .map(|name| ledger::lock_name(name, on_wait))
        .transpose()?;
    let recorded = spec.name().map(ledger::named).transpose()?.flatten();
    let ProviderArgs { provider, options } = args;
    let (provider_name, recorded) = pick_provider(&spec, recorded, provider, file.provider)?;
    let provider = adapters::provider(&provider_name, options.or(file.providers))?;
    let profile_options = profile_options.or(file.profiles);
    let profiles = profile_names
        .iter()
        .map(|profile| adapters::profile(profile, profile_options.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(&provider_name)?;
    let with_next_step = |error| next_step(error, &provider_name, &spec);
    let vm = lifecycle::start(provider.as_ref(), &ledger, &profiles, &spec, recorded)
        .map_err(with_next_step)?;
    drop(lock);
    if detach {
        println!("{vm}");
        if spec.is_disposable() {
            eprintln!(
                "{vm} is disposable: `workspace remove {vm}` deletes it, and its provider deletes it once its lifetime runs out."
            );
        }
        return Ok(());
    }
    lifecycle::attach(provider.as_ref(), &vm).map_err(with_next_step)?;
    leave(provider.as_ref(), &ledger, &vm, &spec)
}

fn leave(
    provider: &dyn Provider,
    ledger: &dyn Ledger,
    vm: &VmId,
    spec: &VmSpec,
) -> Result<(), Box<dyn Error>> {
    let Some(name) = spec.name() else {
        lifecycle::remove(provider, ledger, vm)?;
        eprintln!("{vm} was disposable, so it is deleted.");
        return Ok(());
    };
    eprintln!("{name} is still running: `workspace stop {name}` stops it.");
    Ok(())
}

fn next_step(error: CoreError, provider: &str, spec: &VmSpec) -> Box<dyn Error> {
    match error {
        CoreError::Setup { cause } => NeedsSetup {
            provider: provider.to_owned(),
            cause,
        }
        .into(),
        CoreError::Gone { vm, cause } => Vanished {
            name: spec.name().unwrap_or(vm.as_str()).to_owned(),
            vm,
            cause,
        }
        .into(),
        error => error.into(),
    }
}

fn pick_provider(
    spec: &VmSpec,
    recorded: Option<Record>,
    asked: Option<String>,
    default: Option<String>,
) -> Result<(String, Option<VmId>), Box<dyn Error>> {
    match (recorded, asked) {
        (Some(record), Some(asked)) if asked != record.provider => Err(OtherProvider {
            name: spec.name().unwrap_or_default().to_owned(),
            recorded: record.provider,
            asked,
        }
        .into()),
        (Some(record), _) => Ok((record.provider, Some(record.id))),
        (None, asked) => Ok((asked.or(default).ok_or(NotSetUp)?, None)),
    }
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
