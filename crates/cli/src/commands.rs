use std::error::Error;
use std::io::{self, IsTerminal};
use std::path::Path;

use workspace_core::{VmId, VmSpec, lifecycle};

use crate::ProviderArgs;
use crate::adapters;
use crate::config::{self, Config, Providers};
use crate::ledger::{self, FileLedger, Record};

const COLUMN_GAP: &str = "  ";

#[derive(Debug, thiserror::Error)]
#[error("no terminal to open a shell in: pass -d to start the VM without one")]
struct NoTerminal;

#[derive(Debug, thiserror::Error)]
#[error("no provider chosen: run `workspace setup`, or pass --provider")]
struct NotSetUp;

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
    name: &str,
    args: ProviderArgs,
    file: Config,
    profile_names: &[String],
    detach: bool,
) -> Result<(), Box<dyn Error>> {
    let has_terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    if !detach && !has_terminal {
        return Err(NoTerminal.into());
    }
    let spec = VmSpec::new(name)?;
    let lock = ledger::lock_name(spec.name(), || {
        eprintln!("Another `workspace start {name}` is running: waiting for it to finish.");
    })?;
    let recorded = ledger::named(spec.name())?;
    let ProviderArgs { provider, options } = args;
    let (provider_name, recorded) = pick_provider(spec.name(), recorded, provider, file.provider)?;
    let provider = adapters::provider(&provider_name, options.or(file.providers))?;
    let profiles = profile_names
        .iter()
        .map(|profile| adapters::profile(profile))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(&provider_name)?;
    let vm = lifecycle::start(provider.as_ref(), &ledger, &profiles, &spec, recorded)?;
    drop(lock);
    if detach {
        println!("{vm}");
        return Ok(());
    }
    lifecycle::attach(provider.as_ref(), &vm)?;
    eprintln!("{name} is still running: `workspace stop {name}` stops it.");
    Ok(())
}

fn pick_provider(
    name: &str,
    recorded: Option<Record>,
    asked: Option<String>,
    default: Option<String>,
) -> Result<(String, Option<VmId>), Box<dyn Error>> {
    match (recorded, asked) {
        (Some(record), Some(asked)) if asked != record.provider => Err(OtherProvider {
            name: name.to_owned(),
            recorded: record.provider,
            asked,
        }
        .into()),
        (Some(record), _) => Ok((record.provider, Some(record.id))),
        (None, asked) => Ok((asked.or(default).ok_or(NotSetUp)?, None)),
    }
}

pub fn stop(vm: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    lifecycle::stop(provider.as_ref(), &record.id)?;
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
    let records = ledger::records()?;
    let name_width = records.iter().map(|record| record.name.len()).max();
    let id_width = records.iter().map(|record| record.id.as_str().len()).max();
    let (name_width, id_width) = (name_width.unwrap_or(0), id_width.unwrap_or(0));
    for record in &records {
        let (name, id) = (&record.name, record.id.as_str());
        println!(
            "{name:name_width$}{COLUMN_GAP}{id:id_width$}{COLUMN_GAP}{}",
            record.provider
        );
    }
    Ok(())
}
