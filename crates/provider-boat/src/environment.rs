//! The Boat environment that sandboxes start from. Workspace keeps its own,
//! so that a sandbox gets its own boat key and nothing else from the
//! account: no GitHub access, no agent logins, no secrets. That key is
//! scoped to its own sandbox: the sandbox can stop itself, but cannot reach
//! any other.

use std::fmt;

use serde::Deserialize;

use crate::cli;
use crate::error::BoatError;
use crate::options::EnvironmentName;

/// What onboarding sets. The master switch stays off, since it would also
/// withhold the boat key.
const SETTINGS: Toggles = Toggles {
    safe_for_third_parties: false,
    injected: Injected {
        github: false,
        secrets: false,
        agent_logins: false,
        boat_key: true,
    },
};

/// What `boat env list` prints, down to the fields read here.
#[derive(Deserialize)]
struct Listing {
    environments: Vec<Environment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Environment {
    name: String,
    is_default: bool,
    #[serde(flatten)]
    toggles: Toggles,
    /// What reaches a sandbox, once the master switch is applied.
    effective: Injected,
}

/// What an environment is set to inject.
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Toggles {
    /// The master switch: when on, nothing is injected.
    safe_for_third_parties: bool,
    #[serde(flatten)]
    injected: Injected,
}

/// What reaches a sandbox, item by item.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one of Boat's on or off toggles"
)]
struct Injected {
    #[serde(rename = "passGithub")]
    github: bool,
    #[serde(rename = "passSecrets")]
    secrets: bool,
    #[serde(rename = "passAgentsCredentials")]
    agent_logins: bool,
    #[serde(rename = "passSandboxCredentials")]
    boat_key: bool,
}

impl Injected {
    /// Each item that differs from `expected`, with its state here, such as
    /// `GitHub access on`. Empty when nothing differs.
    fn differences(self, expected: Self) -> String {
        let items = [
            ("GitHub access", self.github, expected.github),
            ("secrets", self.secrets, expected.secrets),
            ("agent logins", self.agent_logins, expected.agent_logins),
            ("boat key", self.boat_key, expected.boat_key),
        ];
        let differences: Vec<String> = items
            .iter()
            .filter(|(_, actual, wanted)| actual != wanted)
            .map(|(item, actual, _)| format!("{item} {}", if *actual { "on" } else { "off" }))
            .collect();
        differences.join(", ")
    }
}

/// What onboarding did to the environment.
pub(crate) enum Change {
    Created,
    Reset,
    Kept,
}

impl fmt::Display for Change {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Created => "created now",
            Self::Reset => "reset now",
            Self::Kept => "already set up",
        })
    }
}

/// Fails unless the environment exists and injects what onboarding sets.
/// `boot` calls it, so no sandbox starts from an environment that someone
/// changed since.
pub(crate) fn check(name: &EnvironmentName) -> Result<(), BoatError> {
    let environment = find(name)?.ok_or_else(|| missing(name))?;
    let changed = environment.effective.differences(SETTINGS.injected);
    if changed.is_empty() {
        return Ok(());
    }
    Err(BoatError::Mismatch {
        name: name.clone(),
        changed,
    })
}

/// Creates the environment when it is missing, and resets what it injects
/// when that differs from [`SETTINGS`]. Boat's default environment is left
/// alone, since every other sandbox starts from it.
pub(crate) fn ensure(name: &EnvironmentName) -> Result<Change, BoatError> {
    let (environment, is_new) = match find(name)? {
        Some(environment) => (environment, false),
        None => (create(name)?, true),
    };
    if environment.is_default {
        return Err(BoatError::DefaultEnvironment { name: name.clone() });
    }
    let is_set = environment.toggles == SETTINGS;
    if !is_set {
        configure(name)?;
    }
    check(name)?;
    Ok(match (is_new, is_set) {
        (true, _) => Change::Created,
        (false, false) => Change::Reset,
        (false, true) => Change::Kept,
    })
}

fn find(name: &EnvironmentName) -> Result<Option<Environment>, BoatError> {
    let listing: Listing = cli::call("env list", &[])?;
    let mut environments = listing.environments.into_iter();
    Ok(environments.find(|environment| environment.name == name.as_str()))
}

/// Boat creates an environment that injects everything. No sandbox starts
/// from it until [`configure`] has run, since `boot` checks first.
fn create(name: &EnvironmentName) -> Result<Environment, BoatError> {
    cli::run("env new", &[name.as_str()])?;
    find(name)?.ok_or_else(|| missing(name))
}

/// Sets every toggle to [`SETTINGS`]. Boat records this as a new version of
/// the environment, which later sandboxes start from.
fn configure(name: &EnvironmentName) -> Result<(), BoatError> {
    let Toggles {
        safe_for_third_parties,
        injected,
    } = SETTINGS;
    let args = [
        name.as_str(),
        "--safe-for-third-parties",
        flag(safe_for_third_parties),
        "--github",
        flag(injected.github),
        "--secrets",
        flag(injected.secrets),
        "--agents-credentials",
        flag(injected.agent_logins),
        "--boat-credentials",
        flag(injected.boat_key),
    ];
    cli::run("env set", &args)?;
    Ok(())
}

fn flag(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn missing(name: &EnvironmentName) -> BoatError {
    BoatError::NoEnvironment { name: name.clone() }
}
