use std::fmt;

use serde::Deserialize;

use crate::cli;
use crate::error::BoatError;
use crate::options::EnvironmentName;

/// The master switch stays off, since it would also withhold the boat key.
const SETTINGS: Toggles = Toggles {
    safe_for_third_parties: false,
    injected: Injected {
        github: false,
        secrets: false,
        agent_logins: false,
        boat_key: true,
    },
};

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
    effective: Injected,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Toggles {
    safe_for_third_parties: bool,
    #[serde(flatten)]
    injected: Injected,
}

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

fn create(name: &EnvironmentName) -> Result<Environment, BoatError> {
    cli::run("env new", &[name.as_str()])?;
    find(name)?.ok_or_else(|| missing(name))
}

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
