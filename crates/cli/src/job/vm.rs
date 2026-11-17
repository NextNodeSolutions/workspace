use std::error::Error;

use workspace_core::port::Provider;
use workspace_core::{VmId, VmSpec, lifecycle};

use crate::adapters;
use crate::config::Config;
use crate::ledger::{self, FileLedger, NameLock};
use crate::start::{NAME_WAIT_MESSAGE, next_step, pick_provider};

use super::{Job, JobError};

pub(super) struct Running {
    pub(super) provider: Box<dyn Provider>,
    pub(super) vm: VmId,
    spec: VmSpec,
    ledger: FileLedger,
    _lock: Option<NameLock>,
}

pub(super) fn start(job: &Job, file: Config) -> Result<Running, Box<dyn Error>> {
    let spec = job
        .name
        .as_deref()
        .map_or_else(|| Ok(VmSpec::disposable()), VmSpec::named)?;
    let lock = spec
        .name()
        .map(|name| {
            ledger::lock_name(name, || {
                eprintln!("{NAME_WAIT_MESSAGE}");
            })
        })
        .transpose()?;
    let recorded = spec.name().map(ledger::named).transpose()?.flatten();
    let (provider_name, recorded) = pick_provider(
        &spec,
        recorded,
        job.provider.provider.clone(),
        file.provider,
    )?;
    let provider = adapters::provider(
        &provider_name,
        job.provider.options.clone().or(file.providers),
    )?;
    let profile_options = job.profile_options.clone().or(file.profiles);
    let profiles = job
        .profiles
        .iter()
        .map(|name| adapters::profile(name, profile_options.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = FileLedger::for_provider(&provider_name)?;
    let vm = lifecycle::start(provider.as_ref(), &ledger, &profiles, &spec, recorded)
        .map_err(|error| start_error(error, &provider_name, &spec))?;
    Ok(Running {
        provider,
        vm,
        spec,
        ledger,
        _lock: lock,
    })
}

impl Running {
    pub(super) fn recovery(&self) -> String {
        recovery(&self.spec, &self.vm)
    }

    pub(super) fn put_away(&self) -> Result<(), JobError> {
        let outcome = if self.spec.is_disposable() {
            lifecycle::remove(self.provider.as_ref(), &self.ledger, &self.vm)
        } else {
            lifecycle::stop(self.provider.as_ref(), &self.vm)
        };
        outcome.map_err(|error| JobError::Cleanup {
            vm: self.vm.clone(),
            next: self.recovery(),
            cause: error,
        })
    }
}

fn start_error(error: workspace_core::Error, provider: &str, spec: &VmSpec) -> Box<dyn Error> {
    use workspace_core::Error as CoreError;
    if let CoreError::Record { vm, .. } = &error {
        return JobError::Unrecorded {
            vm: vm.clone(),
            provider: provider.to_owned(),
            cause: error,
        }
        .into();
    }
    let vm = match &error {
        CoreError::Resume { vm, .. }
        | CoreError::Shell { vm, .. }
        | CoreError::Provision { vm, .. } => Some(vm.clone()),
        _ => None,
    };
    let cause = next_step(error, provider, spec);
    match vm {
        Some(vm) => JobError::Preserved {
            next: recovery(spec, &vm),
            vm,
            cause,
        }
        .into(),
        None => cause,
    }
}

fn recovery(spec: &VmSpec, vm: &VmId) -> String {
    spec.name().map_or_else(
        || format!("workspace remove {vm}"),
        |name| format!("workspace stop {name}"),
    )
}
