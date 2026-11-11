use std::fs;
use std::path::Path;

use clap::Args;
use serde::Deserialize;
use serde_json::{Map, Value};
use workspace_core::Cause;
use workspace_core::port::{Profile, Shell};

use crate::error::AgentError;
use crate::infisical::Secret;
use crate::options::{Identifier, Version};
use crate::secrets::{SecretSettings, SecretSpec};
use crate::{json, local, pi_auth, pi_scripts, remote, vault};

const NAME: &str = "pi";

const DIRECTORY: &str = ".pi/agent";

/// Names the vault project, environment, paths and identities that
/// `bin/provider-key.py` and `bin/pi-token-pull` read, never a value.
const BINDING: &str = ".config/agent-secrets/core-vault.json";

const DEFAULT_SECRETS: &[&str] = &["GITHUB_TOKEN"];

const ADAPTER_SETTINGS: &str = "mcp-adapter.json";

const AUTH_FILE: &str = "auth.json";

/// The VM has no keychain, so the MCP adapter keeps OAuth sign-ins in an
/// encrypted file instead, under a key that stays on the VM.
const OAUTH_STORE_SETTING: (&str, &str) = ("oauthCredentialStore", "encrypted-file");

#[derive(Args, Clone, Default, Deserialize)]
#[command(next_help_heading = "Pi profile")]
#[serde(deny_unknown_fields)]
pub struct PiOptions {
    /// Pi version to install on the VM [default: the version the agent
    /// repository pins]
    #[arg(id = "pi_version", long = "pi-version", value_name = "VERSION")]
    version: Option<Version>,

    /// Infisical project the secrets come from
    #[arg(
        id = "pi_infisical_project",
        long = "pi-infisical-project",
        value_name = "ID"
    )]
    infisical_project: Option<Identifier>,

    /// Infisical environment the secrets come from [default: dev]
    #[arg(
        id = "pi_infisical_env",
        long = "pi-infisical-env",
        value_name = "NAME"
    )]
    infisical_env: Option<Identifier>,

    /// Secrets to export in every shell on the VM, comma-separated
    /// [default: `GITHUB_TOKEN`]
    #[arg(
        id = "pi_secrets",
        long = "pi-secrets",
        value_name = "NAMES",
        value_delimiter = ','
    )]
    secrets: Option<Vec<SecretSpec>>,
}

impl PiOptions {
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            version: self.version.or(fallback.version),
            infisical_project: self.infisical_project.or(fallback.infisical_project),
            infisical_env: self.infisical_env.or(fallback.infisical_env),
            secrets: self.secrets.or(fallback.secrets),
        }
    }
}

pub struct Pi {
    version: Option<Version>,
    secrets: SecretSettings,
}

struct Prepared {
    binding: Vec<u8>,
    adapter_settings: Vec<u8>,
    auth: Vec<u8>,
    secrets: Vec<Secret>,
}

impl Pi {
    #[must_use]
    pub fn new(options: PiOptions) -> Self {
        Self {
            version: options.version,
            secrets: SecretSettings::new(
                options.infisical_project,
                options.infisical_env,
                options.secrets,
                DEFAULT_SECRETS,
            ),
        }
    }

    fn gather(&self) -> Result<Prepared, AgentError> {
        let mut secrets = self.secrets.fetch(NAME)?;
        let home = local::home()?;
        let directory = home.join(DIRECTORY);
        let binding_path = home.join(BINDING);
        let binding = read(&binding_path)?;
        secrets.extend(vault::mint(&binding_path)?);
        Ok(Prepared {
            binding,
            adapter_settings: adapter_settings(&directory.join(ADAPTER_SETTINGS))?,
            auth: pi_auth::auth_file(&directory.join(AUTH_FILE))?,
            secrets,
        })
    }
}

impl Profile for Pi {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Everything local is gathered first, so a missing secret or file fails
    /// before the VM changes. The checkout comes before the install, since
    /// the repository pins the Pi version.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause> {
        let prepared = self.gather()?;
        remote::hook(shell)?;
        shell.run(pi_scripts::CHECKOUT, &[])?;
        shell.run(&pi_scripts::install(self.version.as_ref()), &[])?;
        shell.run(pi_scripts::INFISICAL_INSTALL, &[])?;
        remote::write_file(shell, BINDING, &prepared.binding)?;
        remote::write_secret_file(
            shell,
            &format!("{DIRECTORY}/{ADAPTER_SETTINGS}"),
            &prepared.adapter_settings,
        )?;
        remote::write_secret_file(shell, &format!("{DIRECTORY}/{AUTH_FILE}"), &prepared.auth)?;
        shell.run(pi_scripts::FORGET_SIGN_INS, &[])?;
        shell.run(pi_scripts::SETUP, &[])?;
        shell.run(pi_scripts::OAUTH_KEY, &[])?;
        remote::export_secrets(shell, &prepared.secrets)?;
        shell.run(pi_scripts::CODEX_PULL, &[])?;
        Ok(())
    }
}

fn adapter_settings(path: &Path) -> Result<Vec<u8>, AgentError> {
    json::rewrite(path, |fields| {
        let settings = fields
            .entry("settings")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(settings) = settings {
            let (key, value) = OAUTH_STORE_SETTING;
            settings.insert(key.to_owned(), Value::String(value.to_owned()));
        }
    })
}

fn read(path: &Path) -> Result<Vec<u8>, AgentError> {
    fs::read(path).map_err(|source| AgentError::Read {
        path: path.to_owned(),
        source,
    })
}
