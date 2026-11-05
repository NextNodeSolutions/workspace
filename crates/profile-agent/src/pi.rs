use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use clap::Args;
use serde::Deserialize;
use serde_json::{Map, Value};
use workspace_core::Cause;
use workspace_core::port::{Profile, Shell};

use crate::error::AgentError;
use crate::infisical::Secret;
use crate::options::{Identifier, Version};
use crate::secrets::{SecretSettings, SecretSpec};
use crate::{archive, json, local, pi_auth, pi_scripts, remote, vault};

const NAME: &str = "pi";

const DIRECTORY: &str = ".pi/agent";

/// Pi's secret-output extension imports the policy it shares with Claude's
/// hook from `~/.config/agent-security`, and its provider keys resolve
/// through the vault binding in `~/.config/agent-secrets`, so a VM needs
/// both directories too.
const SHARED_PARENT: &str = ".config";

const SHARED: &[&str] = &["agent-security", "agent-secrets"];

const BINDING: &str = ".config/agent-secrets/core-vault.json";

/// The same program keeps this machine's and the VM's Codex token current.
const PULL_SCRIPT: &str = ".local/bin/pi-token-pull";

const DEFAULT_SECRETS: &[&str] = &["GITHUB_TOKEN"];

const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

const ADAPTER_SETTINGS: &str = "mcp-adapter.json";

const AUTH_FILE: &str = "auth.json";

/// The VM has no keychain, so the MCP adapter keeps OAuth sign-ins in an
/// encrypted file instead, under a key that stays on the VM.
const OAUTH_STORE_SETTING: (&str, &str) = ("oauthCredentialStore", "encrypted-file");

/// What Pi reads from its agent directory, as opposed to what it writes
/// there: sessions, caches, trust decisions and installed dependencies stay.
/// The auth file is written apart, without this machine's OAuth sign-ins.
const MIRRORED: &[&str] = &[
    "AGENTS.md",
    "ARCHITECTURE.md",
    "agents",
    "bin",
    "context-budget.json",
    "extensions",
    "frontend-check.json",
    "keybindings.json",
    "lazy-load.json",
    "mcp-adapter.json",
    "mcp.json",
    "models.json",
    "npm",
    "oxfmt.config.ts",
    "oxlint.config.ts",
    "package.json",
    "packages",
    "plans",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "prompts",
    "settings.json",
    "skills",
    "skills.json",
    "themes",
    "tsconfig.json",
];

#[derive(Args, Clone, Default, Deserialize)]
#[command(next_help_heading = "Pi profile")]
#[serde(deny_unknown_fields)]
pub struct PiOptions {
    /// Pi version to install on the VM [default: the version installed on
    /// this machine]
    #[arg(long = "pi-version", value_name = "VERSION")]
    version: Option<Version>,

    /// Infisical project the secrets come from
    #[arg(long = "pi-infisical-project", value_name = "ID")]
    infisical_project: Option<Identifier>,

    /// Infisical environment the secrets come from [default: dev]
    #[arg(long = "pi-infisical-env", value_name = "NAME")]
    infisical_env: Option<Identifier>,

    /// Secrets to export in every shell on the VM, comma-separated
    /// [default: `GITHUB_TOKEN`]
    #[arg(long = "pi-secrets", value_name = "NAMES", value_delimiter = ',')]
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
    version: Version,
    archive: Vec<u8>,
    shared: Vec<u8>,
    adapter_settings: Vec<u8>,
    auth: Vec<u8>,
    pull_script: Vec<u8>,
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
        let version = match &self.version {
            Some(version) => version.clone(),
            None => local_version()?,
        };
        let mut secrets = self.secrets.fetch(NAME)?;
        let home = local::home()?;
        let directory = home.join(DIRECTORY);
        let shared_parent = home.join(SHARED_PARENT);
        for entry in SHARED {
            let shared = shared_parent.join(entry);
            if !shared.is_dir() {
                return Err(AgentError::MissingShared(shared));
            }
        }
        secrets.extend(vault::mint(&home.join(BINDING))?);
        Ok(Prepared {
            version,
            archive: archive::pack(&directory, MIRRORED)?,
            shared: archive::pack(&shared_parent, SHARED)?,
            adapter_settings: adapter_settings(&directory.join(ADAPTER_SETTINGS))?,
            auth: pi_auth::auth_file(&directory.join(AUTH_FILE))?,
            pull_script: pull_script(&home.join(PULL_SCRIPT))?,
            secrets,
        })
    }
}

impl Profile for Pi {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Everything local is gathered first, so a missing secret or file fails
    /// before the VM changes.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause> {
        let prepared = self.gather()?;
        remote::hook(shell)?;
        shell.run(&pi_scripts::install(&prepared.version), &[])?;
        shell.run(pi_scripts::INFISICAL_INSTALL, &[])?;
        remote::extract(shell, DIRECTORY, &prepared.archive)?;
        remote::extract(shell, SHARED_PARENT, &prepared.shared)?;
        remote::write_file(
            shell,
            &format!("{DIRECTORY}/{ADAPTER_SETTINGS}"),
            &prepared.adapter_settings,
        )?;
        remote::write_secret_file(shell, &format!("{DIRECTORY}/{AUTH_FILE}"), &prepared.auth)?;
        shell.run(pi_scripts::FORGET_SIGN_INS, &[])?;
        shell.run(pi_scripts::DEPENDENCIES, &[])?;
        shell.run(pi_scripts::OAUTH_KEY, &[])?;
        remote::export_secrets(shell, &prepared.secrets)?;
        remote::write_program(shell, PULL_SCRIPT, &prepared.pull_script)?;
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

fn pull_script(path: &Path) -> Result<Vec<u8>, AgentError> {
    fs::read(path).map_err(|_| AgentError::MissingPullScript(path.to_owned()))
}

fn local_version() -> Result<Version, AgentError> {
    let mut command = Command::new(NAME);
    command.arg("--version");
    let stdout = local::run(NAME, command, VERSION_TIMEOUT).map_err(|error| match error {
        AgentError::Spawn { .. } => AgentError::NoLocalPi,
        error => error,
    })?;
    let text = String::from_utf8_lossy(&stdout).trim().to_owned();
    text.parse().map_err(|_| AgentError::LocalVersion(text))
}
