//! Composition root: reads the command line, picks the adapters, calls the core.

mod adapters;
mod ledger;

use std::error::Error;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ledger::FileLedger;
use workspace_core::{VmId, VmSpec, lifecycle};

#[derive(Parser)]
#[command(about = "Boot, close and remove VMs on any provider")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Boot a VM, record it, provision it with each profile, print its id
    Boot {
        name: String,
        #[arg(long)]
        provider: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Stop a recorded VM on the provider that booted it; it stays recorded
    Close { id: String },
    /// Delete a recorded VM and its data for good, then its record
    Remove { id: String },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(error.as_ref());
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), Box<dyn Error>> {
    match command {
        Command::Boot {
            name,
            provider,
            profiles,
        } => boot(&name, &provider, &profiles),
        Command::Close { id } => close(&VmId::from(id)),
        Command::Remove { id } => remove(&VmId::from(id)),
    }
}

fn boot(name: &str, provider_name: &str, profile_names: &[String]) -> Result<(), Box<dyn Error>> {
    let spec = VmSpec::new(name)?;
    let provider = adapters::provider(provider_name)?;
    let profiles = profile_names
        .iter()
        .map(|profile| adapters::profile(profile))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(provider_name)?;
    let vm = lifecycle::boot(provider.as_ref(), &ledger, &profiles, &spec)?;
    println!("{vm}");
    Ok(())
}

fn close(vm: &VmId) -> Result<(), Box<dyn Error>> {
    let ledger = FileLedger::find(vm)?;
    let provider = adapters::provider(ledger.provider())?;
    lifecycle::close(provider.as_ref(), vm)?;
    Ok(())
}

fn remove(vm: &VmId) -> Result<(), Box<dyn Error>> {
    let ledger = FileLedger::find(vm)?;
    let provider = adapters::provider(ledger.provider())?;
    lifecycle::remove(provider.as_ref(), &ledger, vm)?;
    Ok(())
}

fn report(error: &dyn Error) {
    eprintln!("error: {error}");
    for cause in std::iter::successors(error.source(), |&cause| cause.source()) {
        eprintln!("  caused by: {cause}");
    }
}
