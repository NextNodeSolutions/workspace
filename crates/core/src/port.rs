use crate::{Cause, VmId, VmSpec};

pub trait Provider {
    /// # Errors
    ///
    /// When the account is not ready for `create`.
    fn setup(&self) -> Result<String, Cause>;

    /// # Errors
    ///
    /// When no VM was started.
    fn create(&self, spec: &VmSpec) -> Result<VmId, Cause>;

    /// # Errors
    ///
    /// When the VM may not run again, for example because the provider no
    /// longer has it. A request the provider never answered may still start
    /// the VM.
    fn resume(&self, vm: &VmId) -> Result<(), Cause>;

    /// # Errors
    ///
    /// When the VM is not running or does not become reachable.
    fn shell(&self, vm: &VmId) -> Result<Box<dyn Shell + '_>, Cause>;

    /// # Errors
    ///
    /// When no session could be opened, for example because the VM stopped.
    /// The VM is left as it was.
    fn attach(&self, vm: &VmId) -> Result<(), Cause>;

    /// # Errors
    ///
    /// When the VM may still be running.
    fn stop(&self, vm: &VmId) -> Result<(), Cause>;

    /// # Errors
    ///
    /// When the VM may still exist.
    fn remove(&self, vm: &VmId) -> Result<(), Cause>;
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
