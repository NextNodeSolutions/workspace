//! What the core does: prepare a provider, boot a VM ready for its profiles,
//! close it, remove it.

use crate::port::{Ledger, Profile, Provider, Shell};
use crate::{Error, VmId, VmSpec};

/// Prepares the provider account for `boot`, and returns the provider's
/// summary of it.
///
/// # Errors
///
/// [`Error::Onboard`] when the account is not ready.
pub fn onboard(provider: &dyn Provider) -> Result<String, Error> {
    provider.onboard().map_err(|cause| Error::Onboard { cause })
}

/// Boots a VM, records it, waits until it is reachable, then applies
/// `profiles` in order. The record is written before anything waits on the
/// VM, so interrupting a boot never loses a running VM.
///
/// # Errors
///
/// [`Error::Boot`] when the provider fails to start the VM. [`Error::Record`],
/// [`Error::Shell`] and [`Error::Provision`] mean the VM booted but is not
/// ready: it is left running, never closed automatically, and the error names
/// it.
pub fn boot(
    provider: &dyn Provider,
    ledger: &dyn Ledger,
    profiles: &[Box<dyn Profile>],
    spec: &VmSpec,
) -> Result<VmId, Error> {
    let vm = provider.boot(spec).map_err(|cause| Error::Boot {
        name: spec.name().to_owned(),
        cause,
    })?;
    ledger.record(&vm, spec).map_err(|cause| Error::Record {
        vm: vm.clone(),
        cause,
    })?;
    let shell = provider.shell(&vm).map_err(|cause| Error::Shell {
        vm: vm.clone(),
        cause,
    })?;
    provision(shell.as_ref(), profiles, &vm)?;
    Ok(vm)
}

/// Stops a VM. Its record stays, since the VM still exists.
///
/// # Errors
///
/// [`Error::Close`] when the provider fails to stop the VM.
pub fn close(provider: &dyn Provider, vm: &VmId) -> Result<(), Error> {
    provider.close(vm).map_err(|cause| Error::Close {
        vm: vm.clone(),
        cause,
    })
}

/// Deletes a VM for good, then removes its record.
///
/// # Errors
///
/// [`Error::Remove`] when the provider fails to delete the VM: the record is
/// kept so the removal can be retried. [`Error::Forget`] when the VM is
/// deleted but its record remains.
pub fn remove(provider: &dyn Provider, ledger: &dyn Ledger, vm: &VmId) -> Result<(), Error> {
    provider.remove(vm).map_err(|cause| Error::Remove {
        vm: vm.clone(),
        cause,
    })?;
    ledger.forget(vm).map_err(|cause| Error::Forget {
        vm: vm.clone(),
        cause,
    })
}

fn provision(shell: &dyn Shell, profiles: &[Box<dyn Profile>], vm: &VmId) -> Result<(), Error> {
    profiles.iter().try_for_each(|profile| {
        profile.provision(shell).map_err(|cause| Error::Provision {
            profile: profile.name(),
            vm: vm.clone(),
            cause,
        })
    })
}
