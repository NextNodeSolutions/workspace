mod adapters;
mod commands;
mod config;
mod files;
mod ledger;
mod xdg;

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use config::{Profiles, Providers};

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
#[expect(
    clippy::large_enum_variant,
    reason = "one Command is parsed per run, so its size does not matter"
)]
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
        #[command(flatten)]
        profile_options: Profiles,
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
        Command::Setup { provider } => commands::setup(provider, file()?, config_path),
        Command::Start {
            name,
            detach,
            profiles,
            provider,
            profile_options,
        } => commands::start(&name, provider, profile_options, file()?, &profiles, detach),
        Command::Stop { vm } => commands::stop(&vm, file()?.providers),
        Command::Remove { vm } => commands::remove(&vm, file()?.providers),
        Command::List => commands::list(),
    }
}

fn report(error: &dyn Error) {
    eprintln!("error: {error}");
    for cause in std::iter::successors(error.source(), |&cause| cause.source()) {
        eprintln!("  caused by: {cause}");
    }
}
