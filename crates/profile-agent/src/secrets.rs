use std::env;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::error::AgentError;
use crate::infisical::{self, Secret, Source};
use crate::options::{Identifier, InvalidOption, SecretName};

const DEFAULT_ENVIRONMENT: &str = "dev";

const LOCAL_PREFIX: &str = "env:";

/// `NAME` is read from Infisical, and `env:NAME` from this machine's environment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct SecretSpec {
    is_local: bool,
    name: SecretName,
}

impl FromStr for SecretSpec {
    type Err = InvalidOption;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (is_local, name) = match text.strip_prefix(LOCAL_PREFIX) {
            Some(name) => (true, name),
            None => (false, text),
        };
        Ok(Self {
            is_local,
            name: name.parse()?,
        })
    }
}

impl TryFrom<String> for SecretSpec {
    type Error = InvalidOption;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for SecretSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_local {
            formatter.write_str(LOCAL_PREFIX)?;
        }
        write!(formatter, "{}", self.name)
    }
}

pub(crate) struct SecretSettings {
    project: Option<Identifier>,
    environment: Identifier,
    specs: Vec<SecretSpec>,
}

impl SecretSettings {
    pub(crate) fn new(
        project: Option<Identifier>,
        environment: Option<Identifier>,
        specs: Option<Vec<SecretSpec>>,
        defaults: &[&str],
    ) -> Self {
        let default_specs = || {
            defaults
                .iter()
                .map(|spec| spec.parse().expect("default secret names are valid"))
                .collect()
        };
        Self {
            project,
            environment: environment
                .unwrap_or_else(|| DEFAULT_ENVIRONMENT.parse().expect("dev is an identifier")),
            specs: specs.unwrap_or_else(default_specs),
        }
    }

    /// Infisical is called only when a secret comes from it, so a profile
    /// that exports none, or only local ones, needs no project.
    pub(crate) fn fetch(&self, profile: &'static str) -> Result<Vec<Secret>, AgentError> {
        let (local, remote): (Vec<_>, Vec<_>) = self.specs.iter().partition(|spec| spec.is_local);
        let mut secrets = local
            .into_iter()
            .map(|spec| from_environment(&spec.name))
            .collect::<Result<Vec<_>, _>>()?;
        if remote.is_empty() {
            return Ok(secrets);
        }
        let project = self
            .project
            .clone()
            .ok_or(AgentError::NoProject { profile })?;
        let source = Source {
            project,
            environment: self.environment.clone(),
            names: remote.into_iter().map(|spec| spec.name.clone()).collect(),
        };
        secrets.extend(infisical::fetch(&source)?);
        Ok(secrets)
    }
}

fn from_environment(name: &SecretName) -> Result<Secret, AgentError> {
    let value = env::var(name.as_str()).map_err(|_| AgentError::MissingEnvironment {
        name: name.to_string(),
    })?;
    Ok(Secret {
        name: name.clone(),
        value,
    })
}
