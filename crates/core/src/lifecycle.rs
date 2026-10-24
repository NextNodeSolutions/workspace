use crate::port::{Ledger, Profile, Provider, Shell};
use crate::{Error, Failure, Reason, VmId, VmSpec};

const DISPOSABLE: &str = "a disposable VM";

/// # Errors
///
/// [`Error::Setup`] when the account is not ready.
pub fn setup(provider: &dyn Provider) -> Result<String, Error> {
    provider
        .setup()
        .map_err(|failure| failure.into_error(None, |cause| Error::Setup { cause }))
}

/// # Errors
///
/// [`Error::Setup`] when the account must be set up again first.
/// [`Error::Create`] when the provider fails to create the VM, and
/// [`Error::Resume`] when it fails to resume the recorded one.
/// [`Error::Gone`] when the provider no longer has the VM.
/// [`Error::Record`], [`Error::Shell`] and [`Error::Provision`] mean the VM
/// runs but is not ready: it is left running, never stopped automatically,
/// and the error names it.
pub fn start(
    provider: &dyn Provider,
    ledger: &dyn Ledger,
    profiles: &[Box<dyn Profile>],
    spec: &VmSpec,
    recorded: Option<VmId>,
) -> Result<VmId, Error> {
    let vm = match recorded {
        Some(vm) => resume(provider, vm)?,
        None => create(provider, ledger, spec)?,
    };
    let shell = provider.shell(&vm).map_err(|failure| {
        failure.into_error(Some(&vm), |cause| Error::Shell {
            vm: vm.clone(),
            cause,
        })
    })?;
    provision(shell.as_ref(), profiles, &vm)?;
    Ok(vm)
}

/// # Errors
///
/// [`Error::Attach`] when no session could be opened, and [`Error::Gone`]
/// when the provider no longer has the VM.
pub fn attach(provider: &dyn Provider, vm: &VmId) -> Result<(), Error> {
    provider.attach(vm).map_err(|failure| {
        failure.into_error(Some(vm), |cause| Error::Attach {
            vm: vm.clone(),
            cause,
        })
    })
}

/// # Errors
///
/// [`Error::Snapshot`] when nothing is saved under `name`, and
/// [`Error::Gone`] when the provider no longer has the VM.
pub fn snapshot(provider: &dyn Provider, vm: &VmId, name: &str) -> Result<(), Error> {
    provider.snapshot(vm, name).map_err(|failure| {
        failure.into_error(Some(vm), |cause| Error::Snapshot {
            vm: vm.clone(),
            name: name.to_owned(),
            cause,
        })
    })
}

/// # Errors
///
/// [`Error::Stop`] when the provider fails to stop the VM. A VM the provider
/// no longer has counts as stopped.
pub fn stop(provider: &dyn Provider, vm: &VmId) -> Result<(), Error> {
    unless_gone(provider.stop(vm)).map_err(|failure| {
        failure.into_error(Some(vm), |cause| Error::Stop {
            vm: vm.clone(),
            cause,
        })
    })
}

/// # Errors
///
/// [`Error::Remove`] when the provider fails to delete the VM: the record is
/// kept so the removal can be retried. A VM the provider no longer has counts
/// as deleted. [`Error::Forget`] when the VM is deleted but its record
/// remains.
pub fn remove(provider: &dyn Provider, ledger: &dyn Ledger, vm: &VmId) -> Result<(), Error> {
    unless_gone(provider.remove(vm)).map_err(|failure| {
        failure.into_error(Some(vm), |cause| Error::Remove {
            vm: vm.clone(),
            cause,
        })
    })?;
    ledger.forget(vm).map_err(|cause| Error::Forget {
        vm: vm.clone(),
        cause,
    })
}

fn create(provider: &dyn Provider, ledger: &dyn Ledger, spec: &VmSpec) -> Result<VmId, Error> {
    let vm = provider.create(spec).map_err(|failure| {
        failure.into_error(None, |cause| Error::Create {
            name: spec.name().unwrap_or(DISPOSABLE).to_owned(),
            cause,
        })
    })?;
    ledger.record(&vm, spec).map_err(|cause| Error::Record {
        vm: vm.clone(),
        cause,
    })?;
    Ok(vm)
}

fn resume(provider: &dyn Provider, vm: VmId) -> Result<VmId, Error> {
    match provider.resume(&vm) {
        Ok(()) => Ok(vm),
        Err(failure) => Err(failure.into_error(Some(&vm), |cause| Error::Resume {
            vm: vm.clone(),
            cause,
        })),
    }
}

fn unless_gone(outcome: Result<(), Failure>) -> Result<(), Failure> {
    match outcome {
        Err(failure) if failure.reason == Reason::Gone => Ok(()),
        outcome => outcome,
    }
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
