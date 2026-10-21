mod adapters;
mod config;
mod ledger;
mod xdg;

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use adapters::NoProvider;
use clap::{Args, Parser, Subcommand};
use config::{Config, Providers};
use ledger::FileLedger;
use workspace_core::{VmId, VmSpec, lifecycle};

#[derive(Parser)]
#[command(about = "Boot, close and remove VMs on any provider")]
struct Cli {
    /// Read options from this file [default: ~/.config/workspace/config.toml]
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prepare a provider account for boot, make it the default provider and
    /// say what boot will do; safe to run again
    Setup {
        #[command(flatten)]
        provider: ProviderArgs,
    },
    /// Boot a VM, record it, provision it with each profile, print its id
    Boot {
        /// 1 to 63 lowercase letters, digits or hyphens
        name: String,
        /// Profile to apply once the VM answers; repeat it to apply several in order
        #[arg(long = "profile", value_name = "PROFILE")]
        profiles: Vec<String>,
        #[command(flatten)]
        provider: ProviderArgs,
    },
    /// Stop a recorded VM on the provider that booted it; it stays recorded
    Close { id: String },
    /// Delete a recorded VM and its data for good, then its record
    Remove { id: String },
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
    let file = config::load(config_path)?;
    match command {
        Command::Setup { provider } => setup(provider, file, config_path),
        Command::Boot {
            name,
            profiles,
            provider,
        } => boot(&name, provider, file, &profiles),
        Command::Close { id } => close(&VmId::from(id), file.providers),
        Command::Remove { id } => remove(&VmId::from(id), file.providers),
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
    let path = config::remember_provider(config_path, &name)?;
    println!("{summary}");
    println!(
        "Commands run on {name} unless --provider names another, as {} sets.",
        path.display()
    );
    Ok(())
}

fn boot(
    name: &str,
    args: ProviderArgs,
    file: Config,
    profile_names: &[String],
) -> Result<(), Box<dyn Error>> {
    let ProviderArgs { provider, options } = args;
    let provider_name = provider.or(file.provider).ok_or(NoProvider::NotSetUp)?;
    let spec = VmSpec::new(name)?;
    let provider = adapters::provider(&provider_name, options.or(file.providers))?;
    let profiles = profile_names
        .iter()
        .map(|profile| adapters::profile(profile))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(&provider_name)?;
    let vm = lifecycle::boot(provider.as_ref(), &ledger, &profiles, &spec)?;
    println!("{vm}");
    Ok(())
}

fn close(vm: &VmId, file: Providers) -> Result<(), Box<dyn Error>> {
    let ledger = FileLedger::find(vm)?;
    let provider = adapters::provider(ledger.provider(), file)?;
    lifecycle::close(provider.as_ref(), vm)?;
    Ok(())
}

fn remove(vm: &VmId, file: Providers) -> Result<(), Box<dyn Error>> {
    let ledger = FileLedger::find(vm)?;
    let provider = adapters::provider(ledger.provider(), file)?;
    lifecycle::remove(provider.as_ref(), &ledger, vm)?;
    Ok(())
}

fn report(error: &dyn Error) {
    eprintln!("error: {error}");
    for cause in std::iter::successors(error.source(), |&cause| cause.source()) {
        eprintln!("  caused by: {cause}");
    }
}
