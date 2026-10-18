//! Composition root: reads the command line, picks the adapters, calls the core.

mod adapters;

use std::error::Error;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use workspace_core::{VmId, VmSpec, lifecycle};

#[derive(Parser)]
#[command(about = "Boot and close VMs on any provider")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Boot a VM, provision it with each profile, print its id
    Boot {
        name: String,
        #[arg(long)]
        provider: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Close a VM
    Close {
        id: String,
        #[arg(long)]
        provider: String,
    },
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
        } => {
            let spec = VmSpec::new(&name)?;
            let provider = adapters::provider(&provider)?;
            let profiles = profiles
                .iter()
                .map(|profile| adapters::profile(profile))
                .collect::<Result<Vec<_>, _>>()?;
            let vm = lifecycle::boot(provider.as_ref(), &profiles, &spec)?;
            println!("{vm}");
        }
        Command::Close { id, provider } => {
            let provider = adapters::provider(&provider)?;
            lifecycle::close(provider.as_ref(), &VmId::from(id))?;
        }
    }
    Ok(())
}

fn report(error: &dyn Error) {
    eprintln!("error: {error}");
    for cause in std::iter::successors(error.source(), |&cause| cause.source()) {
        eprintln!("  caused by: {cause}");
    }
}
