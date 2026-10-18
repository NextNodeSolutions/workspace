//! What the core does: boot a VM ready for its profiles, and close it.

use crate::port::{Profile, Provider, Shell};
use crate::{Error, VmId, VmSpec};

/// Boots a VM, waits until it is reachable, then applies `profiles` in order.
///
/// # Errors
///
/// [`Error::Boot`] when the provider fails to start the VM. [`Error::Shell`]
/// and [`Error::Provision`] mean the VM booted but is not ready: it is left
/// running, never closed automatically, and the error names it.
pub fn boot(
    provider: &dyn Provider,
    profiles: &[Box<dyn Profile>],
    spec: &VmSpec,
) -> Result<VmId, Error> {
    let vm = provider.boot(spec).map_err(|cause| Error::Boot {
        name: spec.name().to_owned(),
        cause,
    })?;
    let shell = provider.shell(&vm).map_err(|cause| Error::Shell {
        vm: vm.clone(),
        cause,
    })?;
    provision(shell.as_ref(), profiles, &vm)?;
    Ok(vm)
}

/// # Errors
///
/// [`Error::Close`] when the provider fails to release the VM.
pub fn close(provider: &dyn Provider, vm: &VmId) -> Result<(), Error> {
    provider.close(vm).map_err(|cause| Error::Close {
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
