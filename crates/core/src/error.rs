use crate::VmId;
use crate::vm::MAX_NAME_LENGTH;

/// A failure reported by an adapter. The core attaches it to an [`Error`]
/// without inspecting it, so no provider type leaks into the core.
pub type Cause = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "`{name}` is not a valid vm name: use 1 to {max} lowercase letters, digits or hyphens, with no hyphen at either end",
        max = MAX_NAME_LENGTH
    )]
    InvalidName { name: String },

    #[error("could not boot `{name}`")]
    Boot {
        name: String,
        #[source]
        cause: Cause,
    },

    #[error("booted {vm} but could not record it; it is still running")]
    Record {
        vm: VmId,
        #[source]
        cause: Cause,
    },

    #[error("booted {vm} but could not reach it; it is still running")]
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

    #[error("could not close {vm}; it may still be running")]
    Close {
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
