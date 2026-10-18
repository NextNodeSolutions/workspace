//! The traits adapters implement. The core reaches the outside only through them.

use crate::{Cause, VmId, VmSpec};

/// Where VMs run: Boat, AWS, Cloudflare, a local hypervisor.
pub trait Provider {
    /// Starts a VM and returns its id as soon as the provider assigns one, so
    /// a VM that never becomes reachable still has an id the caller can close.
    ///
    /// # Errors
    ///
    /// When no VM was started.
    fn boot(&self, spec: &VmSpec) -> Result<VmId, Cause>;

    /// Opens a shell on the VM, waiting until it is reachable.
    ///
    /// # Errors
    ///
    /// When the VM is not running or does not become reachable.
    fn shell(&self, vm: &VmId) -> Result<Box<dyn Shell + '_>, Cause>;

    /// Stops and releases the VM. Closing a VM that is already gone succeeds.
    ///
    /// # Errors
    ///
    /// When the VM may still be running.
    fn close(&self, vm: &VmId) -> Result<(), Cause>;
}

/// Runs commands inside a VM, whatever the transport (ssh, provider API).
pub trait Shell {
    /// Runs `script` with bash, feeding it `stdin`, and returns its stdout.
    /// Secrets and files travel on `stdin`, never in `script`.
    ///
    /// # Errors
    ///
    /// When the script cannot be started or exits non-zero; the cause carries
    /// its stderr.
    fn run(&self, script: &str, stdin: &[u8]) -> Result<String, Cause>;
}

/// What a VM is for: Pi with its skills, a long-running cron. Applied after
/// boot, the same way on every provider.
pub trait Profile {
    /// Names the profile in errors.
    fn name(&self) -> &'static str;

    /// Installs and configures the profile. Running it again on the same VM
    /// must be safe.
    ///
    /// # Errors
    ///
    /// When a step fails; the VM is left as the failed step found it.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause>;
}

/// Remembers which VMs exist, so an interrupted boot never loses a VM that is
/// still running and billed.
pub trait Ledger {
    /// Records a VM. Called as soon as the provider returns its id, before
    /// anything waits on the VM.
    ///
    /// # Errors
    ///
    /// When the record may not have been written.
    fn record(&self, vm: &VmId, spec: &VmSpec) -> Result<(), Cause>;

    /// Removes the record of a closed VM. Forgetting a VM with no record
    /// succeeds.
    ///
    /// # Errors
    ///
    /// When the record may still exist.
    fn forget(&self, vm: &VmId) -> Result<(), Cause>;
}
