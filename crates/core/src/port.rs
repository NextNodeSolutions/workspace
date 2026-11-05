use std::process::Command;

use crate::{Cause, Failure, VmId, VmSpec};

/// How a session on a VM ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// The program ended: the person left the shell, or the command ran to its end.
    Left { success: bool },
    /// Nothing passed through the terminal for the idle duration, so the
    /// session was ended.
    Idle,
}

pub trait Provider {
    /// # Errors
    ///
    /// When the account is not ready for `create`.
    fn setup(&self) -> Result<String, Failure>;

    /// # Errors
    ///
    /// When no VM was started.
    fn create(&self, spec: &VmSpec) -> Result<VmId, Failure>;

    /// # Errors
    ///
    /// When the VM may not run again, for example because the provider no
    /// longer has it. A request the provider never answered may still start
    /// the VM.
    fn resume(&self, vm: &VmId) -> Result<(), Failure>;

    /// # Errors
    ///
    /// When the VM is not running or does not become reachable.
    fn shell(&self, vm: &VmId) -> Result<Box<dyn Shell + '_>, Failure>;

    /// Opens a session on the VM through `terminal`: a shell when `command`
    /// is empty, else that command.
    ///
    /// # Errors
    ///
    /// When no session could be opened, for example because the VM stopped.
    /// The VM is left as it was.
    fn attach(
        &self,
        vm: &VmId,
        command: &[String],
        terminal: &dyn Terminal,
    ) -> Result<SessionEnd, Failure>;

    /// # Errors
    ///
    /// When the VM may still be running.
    fn stop(&self, vm: &VmId) -> Result<(), Failure>;

    /// # Errors
    ///
    /// When nothing is saved under `name`. The VM is left as it was.
    fn snapshot(&self, vm: &VmId, name: &str) -> Result<(), Failure>;

    /// # Errors
    ///
    /// When the VM may still exist.
    fn remove(&self, vm: &VmId) -> Result<(), Failure>;
}

/// This machine's terminal, which runs the program a provider gives to open
/// a session, and watches it for idleness.
pub trait Terminal {
    /// # Errors
    ///
    /// When the program could not run on this terminal.
    fn run(&self, program: Command) -> Result<SessionEnd, Cause>;
}

pub trait Shell {
    /// # Errors
    ///
    /// When the script cannot be started or exits non-zero; the cause carries
    /// its stderr.
    fn run(&self, script: &str, stdin: &[u8]) -> Result<String, Cause>;
}

pub trait Profile {
    fn name(&self) -> &'static str;

    /// # Errors
    ///
    /// When a step fails; the VM is left as the failed step found it.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause>;
}

pub trait Ledger {
    /// # Errors
    ///
    /// When the record may not have been written.
    fn record(&self, vm: &VmId, spec: &VmSpec) -> Result<(), Cause>;

    /// # Errors
    ///
    /// When the record may still exist.
    fn forget(&self, vm: &VmId) -> Result<(), Cause>;
}
