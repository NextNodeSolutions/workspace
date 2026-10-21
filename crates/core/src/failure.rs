use crate::{Cause, Error, VmId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NotSetUp,
    Gone,
    Other,
}

#[derive(Debug)]
pub struct Failure {
    pub(crate) reason: Reason,
    cause: Cause,
}

impl Failure {
    #[must_use]
    pub fn new(reason: Reason, cause: impl Into<Cause>) -> Self {
        Self {
            reason,
            cause: cause.into(),
        }
    }

    pub(crate) fn into_error(self, vm: Option<&VmId>, step: impl FnOnce(Cause) -> Error) -> Error {
        let Self { reason, cause } = self;
        match (reason, vm) {
            (Reason::NotSetUp, _) => Error::Setup { cause },
            (Reason::Gone, Some(vm)) => Error::Gone {
                vm: vm.clone(),
                cause,
            },
            _ => step(cause),
        }
    }
}
