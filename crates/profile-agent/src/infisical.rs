use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::error::AgentError;
use crate::local;
use crate::options::{Identifier, SecretName};

const INFISICAL: &str = "infisical";

const EXPORT_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) struct Source {
    pub(crate) project: Identifier,
    pub(crate) environment: Identifier,
    pub(crate) names: Vec<SecretName>,
}

pub(crate) struct Secret {
    pub(crate) name: SecretName,
    pub(crate) value: String,
}

#[derive(Deserialize)]
struct Exported {
    key: String,
    value: String,
}

pub(crate) fn fetch(source: &Source) -> Result<Vec<Secret>, AgentError> {
    let mut command = Command::new(INFISICAL);
    command.args(["export", "--format", "json", "--silent"]);
    command.args(["--projectId", source.project.as_str()]);
    command.args(["--env", source.environment.as_str()]);
    let stdout = local::run(INFISICAL, command, EXPORT_TIMEOUT)?;
    let exported: Vec<Exported> =
        serde_json::from_slice(&stdout).map_err(AgentError::Unreadable)?;
    let (found, missing): (Vec<_>, Vec<_>) = source
        .names
        .iter()
        .map(|name| {
            let value = exported.iter().find(|secret| secret.key == name.as_str());
            (name, value.map(|secret| secret.value.clone()))
        })
        .partition(|(_, value)| value.is_some());
    if !missing.is_empty() {
        let missing: Vec<&str> = missing.iter().map(|(name, _)| name.as_str()).collect();
        return Err(AgentError::MissingSecrets {
            project: source.project.to_string(),
            environment: source.environment.to_string(),
            missing: missing.join(", "),
        });
    }
    Ok(found
        .into_iter()
        .map(|(name, value)| Secret {
            name: name.clone(),
            value: value.expect("partitioned on presence"),
        })
        .collect())
}
