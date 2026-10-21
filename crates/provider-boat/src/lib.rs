mod cli;
mod environment;
mod error;
mod options;
mod process;
mod sandbox;
mod setup;
mod shell;

use std::time::Duration;

use serde::de::IgnoredAny;
use workspace_core::port::{Provider, Shell};
use workspace_core::{Cause, VmId, VmSpec};

use error::BoatError;
use options::Settings;
use sandbox::State;
use shell::BoatShell;

pub use options::Options;

const START_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Covers the disk snapshot Boat takes before it stops a sandbox.
const STOP_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct Boat {
    settings: Settings,
}

impl Boat {
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            settings: options.resolve(),
        }
    }

    fn start_flags(&self) -> Result<[String; 4], BoatError> {
        let Settings {
            ttl, environment, ..
        } = &self.settings;
        environment::check(environment)?;
        Ok([
            "--ttl".to_owned(),
            ttl.seconds().to_string(),
            "--environment".to_owned(),
            environment.as_str().to_owned(),
        ])
    }
}

impl Provider for Boat {
    fn setup(&self) -> Result<String, Cause> {
        Ok(setup::run(&self.settings)?)
    }

    /// Boat names sandboxes itself, so a VM's name lives only in its record.
    fn create(&self, _spec: &VmSpec) -> Result<VmId, Cause> {
        let flags = self.start_flags()?;
        let mut args = vec!["--type", self.settings.machine_type.as_str()];
        args.extend(flags.iter().map(String::as_str));
        let id = sandbox::create(&args)?;
        Ok(VmId::from(id))
    }

    fn resume(&self, vm: &VmId) -> Result<(), Cause> {
        let state = sandbox::wait_while(vm.as_str(), STOP_TIMEOUT, State::is_stopping)?;
        match state {
            State::Stopped => {
                let flags = self.start_flags()?;
                Ok(sandbox::resume(
                    vm.as_str(),
                    &flags.each_ref().map(String::as_str),
                )?)
            }
            State::Gone => Err(BoatError::Gone.into()),
            state if state.is_running() => Ok(()),
            state => Err(BoatError::NotResumable { state }.into()),
        }
    }

    fn shell(&self, vm: &VmId) -> Result<Box<dyn Shell + '_>, Cause> {
        let state = sandbox::wait_while(vm.as_str(), START_TIMEOUT, State::is_starting)?;
        if state != State::Up {
            return Err(BoatError::NotRunning { state }.into());
        }
        Ok(Box::new(BoatShell::new(vm.clone())))
    }

    fn attach(&self, vm: &VmId) -> Result<(), Cause> {
        Ok(shell::attach(vm)?)
    }

    fn stop(&self, vm: &VmId) -> Result<(), Cause> {
        let request = match cli::call::<IgnoredAny>("stop", &[vm.as_str()]) {
            Err(error) if !error.is_refusal() => return Err(error.into()),
            request => request,
        };
        let state = sandbox::wait_while(vm.as_str(), STOP_TIMEOUT, State::is_stopping)?;
        match (state, request) {
            (State::Stopped | State::Gone, _) => Ok(()),
            (_, Err(refusal)) => Err(refusal.into()),
            (state, Ok(_)) => Err(BoatError::NotStopped { state }.into()),
        }
    }

    fn remove(&self, vm: &VmId) -> Result<(), Cause> {
        match cli::call::<IgnoredAny>("delete", &[vm.as_str(), "--yes"]) {
            Err(error) if !error.is_gone() => Err(error.into()),
            _ => Ok(()),
        }
    }
}
