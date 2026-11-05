use std::error::Error;
use std::io::{self, IsTerminal};

use workspace_core::port::{Ledger, Provider};
use workspace_core::{Cause, Error as CoreError, SessionEnd, VmId, VmSpec, lifecycle};

use crate::ProviderArgs;
use crate::adapters;
use crate::config::{Config, Profiles};
use crate::ledger::{self, FileLedger, Record};
use crate::terminal::{Idle, LocalTerminal};

#[derive(Debug, thiserror::Error)]
#[error("no terminal to open a session in: pass -d to start the VM without one")]
struct NoTerminal;

/// What happens once the VM is ready.
pub struct Session {
    pub detach: bool,
    pub idle: Idle,
    pub command: Vec<String>,
}

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
#[error("`{name}` runs on {recorded}, not {asked}: drop --provider, or pick another name")]
struct OtherProvider {
    name: String,
    recorded: String,
    asked: String,
}

pub fn run(
    name: Option<&str>,
    args: ProviderArgs,
    profile_options: Profiles,
    file: Config,
    profile_names: &[String],
    session: &Session,
) -> Result<(), Box<dyn Error>> {
    let has_terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    if !session.detach && !has_terminal {
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
    if session.detach {
        println!("{vm}");
        if spec.is_disposable() {
            eprintln!(
                "{vm} is disposable: `workspace remove {vm}` deletes it, and its provider deletes it once its lifetime runs out."
            );
        }
        return Ok(());
    }
    let terminal = LocalTerminal::new(session.idle);
    let end = lifecycle::attach(provider.as_ref(), &vm, &session.command, &terminal)
        .map_err(with_next_step)?;
    settle(provider.as_ref(), &ledger, &vm, &spec, session, end)
}

fn settle(
    provider: &dyn Provider,
    ledger: &dyn Ledger,
    vm: &VmId,
    spec: &VmSpec,
    session: &Session,
    end: SessionEnd,
) -> Result<(), Box<dyn Error>> {
    match end {
        SessionEnd::Idle => {
            eprintln!(
                "Nothing passed through the terminal for {}, so the session ended.",
                session.idle
            );
            put_away(provider, ledger, vm, spec)
        }
        SessionEnd::Left { .. } if !session.command.is_empty() => {
            eprintln!("The command ended.");
            put_away(provider, ledger, vm, spec)
        }
        SessionEnd::Left { .. } => leave(provider, ledger, vm, spec),
    }
}

/// What a session that ran its course leaves: a named VM stopped with its
/// disk kept, a disposable one deleted.
fn put_away(
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
    lifecycle::stop(provider, vm)?;
    eprintln!("{name} is stopped and its disk kept: `workspace start {name}` resumes it.");
    Ok(())
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
