mod adapters;
mod config;
mod files;
mod ledger;
mod xdg;

use std::error::Error;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use adapters::NoProvider;
use clap::{Args, Parser, Subcommand};
use config::{Config, Providers};
use ledger::FileLedger;
use workspace_core::{VmSpec, lifecycle};

/// Separates the columns that `list` prints.
const COLUMN_GAP: &str = "  ";

#[derive(Parser)]
#[command(about = "Start, stop and remove VMs on any provider")]
struct Cli {
    /// Read options from this file [default: ~/.config/workspace/config.toml]
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prepare a provider account, make it the default provider and say what
    /// start will do; safe to run again
    Setup {
        #[command(flatten)]
        provider: ProviderArgs,
    },
    /// Create the VM, or resume it when its name is recorded, apply each
    /// profile, then open a shell on it; safe to run again
    Start {
        /// 1 to 63 lowercase letters, digits or hyphens
        name: String,
        /// Print the VM's id instead of opening a shell on it, as scripts need
        #[arg(short, long)]
        detach: bool,
        /// Profile to apply once the VM answers; repeat it to apply several in order
        #[arg(long = "profile", value_name = "PROFILE")]
        profiles: Vec<String>,
        #[command(flatten)]
        provider: ProviderArgs,
    },
    /// Stop a recorded VM on the provider that created it; it stays recorded
    Stop {
        /// The VM's name, or its id
        vm: String,
    },
    /// Delete a recorded VM and its data for good, then its record
    Remove {
        /// The VM's name, or its id
        vm: String,
    },
    /// Print each recorded VM: its name, its id and its provider
    #[command(visible_alias = "ls")]
    List,
}

#[derive(Debug, thiserror::Error)]
#[error("no terminal to open a shell in: pass -d to start the VM without one")]
struct NoTerminal;

#[derive(Debug, thiserror::Error)]
#[error("`{name}` runs on {recorded}, not {asked}: drop --provider, or pick another name")]
struct OtherProvider {
    name: String,
    recorded: String,
    asked: String,
}

#[derive(Args)]
struct ProviderArgs {
    /// Provider to use, such as boat [default: the one `workspace setup` chose]
    #[arg(long)]
    provider: Option<String>,
    #[command(flatten)]
    options: Providers,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command, cli.config.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(error.as_ref());
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command, config_path: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let file = || config::load(config_path);
    match command {
        Command::Setup { provider } => setup(provider, file()?, config_path),
        Command::Start {
            name,
            detach,
            profiles,
            provider,
        } => start(&name, provider, file()?, &profiles, detach),
        Command::Stop { vm } => stop(&vm, file()?.providers),
        Command::Remove { vm } => remove(&vm, file()?.providers),
        // It builds no provider, so a broken config file never hides the VMs.
        Command::List => list(),
    }
}

/// Prepares the provider, then remembers it as the default, so a provider
/// that is not ready never becomes the default.
fn setup(
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
    // Printed first, so that a config file that cannot be written still
    // leaves on screen what setup changed on the account.
    println!("{summary}");
    let path = config::remember_provider(config_path, &name)?;
    println!(
        "Commands run on {name} unless --provider names another, as {} sets.",
        path.display()
    );
    Ok(())
}

/// A recorded VM runs on the provider that created it, so its record picks
/// the provider. A new VM runs on the default one. Without a terminal for
/// the shell, it fails before it creates anything.
fn start(
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
    let recorded = ledger::named(spec.name())?;
    let ProviderArgs { provider, options } = args;
    let provider_name = match (&recorded, provider) {
        (Some(record), Some(asked)) if asked != record.provider => {
            return Err(OtherProvider {
                name: record.name.clone(),
                recorded: record.provider.clone(),
                asked,
            }
            .into());
        }
        (Some(record), _) => record.provider.clone(),
        (None, provider) => provider.or(file.provider).ok_or(NoProvider::NotSetUp)?,
    };
    let provider = adapters::provider(&provider_name, options.or(file.providers))?;
    let profiles = profile_names
        .iter()
        .map(|profile| adapters::profile(profile))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(&provider_name)?;
    let recorded = recorded.map(|record| record.id);
    let vm = lifecycle::start(provider.as_ref(), &ledger, &profiles, &spec, recorded)?;
    if detach {
        println!("{vm}");
        return Ok(());
    }
    lifecycle::attach(provider.as_ref(), &vm)?;
    // The VM costs money until it stops, so leaving the shell says so.
    eprintln!("{name} is still running: `workspace stop {name}` stops it.");
    Ok(())
}

fn stop(vm: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    lifecycle::stop(provider.as_ref(), &record.id)?;
    Ok(())
}

fn remove(vm: &str, file: Providers) -> Result<(), Box<dyn Error>> {
    let record = ledger::find(vm)?;
    let provider = adapters::provider(&record.provider, file)?;
    let ledger = FileLedger::for_provider(&record.provider)?;
    lifecycle::remove(provider.as_ref(), &ledger, &record.id)?;
    Ok(())
}

/// One line per VM, in aligned columns.
fn list() -> Result<(), Box<dyn Error>> {
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

fn report(error: &dyn Error) {
    eprintln!("error: {error}");
    for cause in std::iter::successors(error.source(), |&cause| cause.source()) {
        eprintln!("  caused by: {cause}");
    }
}
