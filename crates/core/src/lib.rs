//! Boots and closes VMs, then provisions them with profiles.
//!
//! The core knows no provider and no profile: adapters implement the traits in
//! [`port`], and the binary chooses which ones to plug in.

mod error;
pub mod lifecycle;
pub mod port;
mod vm;

pub use error::{Cause, Error};
pub use vm::{VmId, VmSpec};
