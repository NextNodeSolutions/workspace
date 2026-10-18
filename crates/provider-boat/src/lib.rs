//! Runs VMs as Boat sandboxes, through the `boat` CLI.
//!
//! The CLI must be on PATH and signed in. Everything it prints is read here
//! and turned into this crate's errors. Nothing reaches the terminal.

mod cli;
mod error;
mod process;
mod sandbox;
mod shell;

use std::time::Duration;

use serde::de::IgnoredAny;
use workspace_core::port::{Provider, Shell};
use workspace_core::{Cause, VmId, VmSpec};

use error::BoatError;
use sandbox::State;
use shell::BoatShell;

/// The operator's sizing rule: `small`, unless someone asks for more.
const MACHINE_TYPE: &str = "small";

/// Boat stops a sandbox this long after it boots, even mid-work. Its timer
/// cannot tell when an agent goes idle, so stopping then is up to the profile.
const HARD_LIMIT: Duration = Duration::from_secs(8 * 60 * 60);

/// How long `shell` waits for a sandbox to come up.
const START_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How long `close` waits for the snapshot Boat takes before a stop.
const STOP_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Boat's VMs. Boat names sandboxes itself, so a VM's name lives only in
/// its record.
pub struct Boat;

impl Provider for Boat {
    fn boot(&self, _spec: &VmSpec) -> Result<VmId, Cause> {
        let ttl = HARD_LIMIT.as_secs().to_string();
        let id = sandbox::create(&["--type", MACHINE_TYPE, "--ttl", &ttl])?;
        Ok(VmId::from(id))
    }

    fn shell(&self, vm: &VmId) -> Result<Box<dyn Shell + '_>, Cause> {
        let state = sandbox::wait_while(vm.as_str(), START_TIMEOUT, State::is_starting)?;
        if state != State::Up {
            return Err(BoatError::NotRunning { state }.into());
        }
        Ok(Box::new(BoatShell::new(vm.clone())))
    }

    /// Boat refuses to stop a sandbox that is already stopped or gone, so
    /// after a refusal the state decides. A call that got no answer at all
    /// fails at once.
    fn close(&self, vm: &VmId) -> Result<(), Cause> {
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
