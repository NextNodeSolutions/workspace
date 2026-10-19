//! Prepares a Boat account for `boot`: the CLI must be signed in, the
//! account must be able to start the configured machine type, and the
//! environment must exist and inject only the sandbox's boat key.

use serde::Deserialize;

use crate::error::BoatError;
use crate::options::{MachineType, Settings};
use crate::{cli, environment};

/// The login state `boat status` reports once the CLI is signed in.
const SIGNED_IN: &str = "active";

const NOT_LISTED: &str = "Boat does not offer it";

/// What `boat status` prints, down to the field read here.
#[derive(Deserialize)]
struct Status {
    account: Option<Account>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    login_state: String,
}

/// What `boat limits` prints, down to the field read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    machine_types: Vec<Offer>,
}

/// Whether the account can start one machine type, and if not, why.
#[derive(Deserialize)]
struct Offer {
    #[serde(rename = "type")]
    name: String,
    available: bool,
    reason: Option<String>,
}

pub(crate) fn run(settings: &Settings) -> Result<String, BoatError> {
    check_signed_in()?;
    check_machine_type(settings.machine_type)?;
    let change = environment::ensure(&settings.environment)?;
    Ok(format!(
        "Boat is ready: `workspace boot` starts {} sandboxes, which Boat stops {} after boot.\n\
         They start from the Boat environment `{}` ({change}), which gives them their own boat key and nothing else from the account.",
        settings.machine_type, settings.ttl, settings.environment
    ))
}

fn check_signed_in() -> Result<(), BoatError> {
    let status: Status = cli::call("status", &[])?;
    let state = status.account.map(|account| account.login_state);
    if state.as_deref() == Some(SIGNED_IN) {
        Ok(())
    } else {
        Err(BoatError::SignedOut)
    }
}

fn check_machine_type(machine_type: MachineType) -> Result<(), BoatError> {
    let limits: Limits = cli::call("limits", &[])?;
    let offer = limits
        .machine_types
        .into_iter()
        .find(|offer| offer.name == machine_type.as_str());
    match offer {
        Some(Offer {
            available: true, ..
        }) => Ok(()),
        offer => Err(BoatError::Unavailable {
            machine_type,
            reason: offer
                .and_then(|offer| offer.reason)
                .unwrap_or_else(|| NOT_LISTED.to_owned()),
        }),
    }
}
