use crate::VmId;
use crate::vm::MAX_NAME_LENGTH;

pub type Cause = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "`{name}` is not a valid vm name: use 1 to {max} lowercase letters, digits or hyphens, with no hyphen at either end",
        max = MAX_NAME_LENGTH
    )]
    InvalidName { name: String },

    #[error("the provider is not ready to start VMs")]
    Setup {
        #[source]
        cause: Cause,
    },

    #[error("could not create `{name}`")]
    Create {
        name: String,
        #[source]
        cause: Cause,
    },

    #[error("created {vm} but could not record it; it is still running")]
    Record {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("could not resume {vm}; its record is kept")]
    Resume {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("started {vm} but could not reach it; it is still running")]
    Shell {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("profile `{profile}` failed on {vm}; it is still running")]
    Provision {
        profile: &'static str,
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("could not open a shell on {vm}; it is left as it was")]
    Attach {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("could not stop {vm}; it may still be running")]
    Stop {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("could not remove {vm}; its record is kept")]
    Remove {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("removed {vm} but could not delete its record")]
    Forget {
        vm: VmId,
        #[source]
        cause: Cause,
    },
}
