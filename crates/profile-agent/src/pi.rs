use std::process::Command;
use std::time::Duration;

use clap::Args;
use serde::Deserialize;
use workspace_core::Cause;
use workspace_core::port::{Profile, Shell};

use crate::error::AgentError;
use crate::options::{Identifier, SecretName, SecretSettings, Version};
use crate::{archive, infisical, local, remote};

const NAME: &str = "pi";

const DIRECTORY: &str = ".pi/agent";

const PACKAGE: &str = "@earendil-works/pi-coding-agent";

const DEFAULT_SECRETS: &[&str] = &["GITHUB_TOKEN"];

const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

/// What Pi reads from its agent directory, as opposed to what it writes
/// there: sessions, caches, trust decisions and installed dependencies stay.
const MIRRORED: &[&str] = &[
    "AGENTS.md",
    "ARCHITECTURE.md",
    "agents",
    "auth.json",
    "bin",
    "context-budget.json",
    "extensions",
    "frontend-check.json",
    "keybindings.json",
    "lazy-load.json",
    "mcp-adapter.json",
    "mcp-auth.json",
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

/// Pi installs its `npm:` packages itself under `npm/`, so that directory
/// is left to it.
const DEPENDENCIES_SCRIPT: &str = r#"set -euo pipefail
cd "$HOME/.pi/agent"
for dir in . packages/*/; do
  [ -f "$dir/pnpm-lock.yaml" ] || continue
  (cd "$dir" && pnpm install --frozen-lockfile --silent --config.strict-dep-builds=false)
done
"#;

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
    secrets: Option<Vec<SecretName>>,
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
}

impl Profile for Pi {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Everything local is gathered first, so a missing secret or file fails
    /// before the VM changes.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause> {
        let version = match &self.version {
            Some(version) => version.clone(),
            None => local_version()?,
        };
        let secrets = match self.secrets.source(NAME)? {
            Some(source) => infisical::fetch(&source)?,
            None => Vec::new(),
        };
        let archive = archive::pack(&local::home()?.join(DIRECTORY), MIRRORED)?;
        remote::hook(shell)?;
        shell.run(&install_script(&version), &[])?;
        remote::extract(shell, DIRECTORY, &archive)?;
        shell.run(DEPENDENCIES_SCRIPT, &[])?;
        if !secrets.is_empty() {
            remote::export_secrets(shell, &secrets)?;
        }
        Ok(())
    }
}

fn install_script(version: &Version) -> String {
    let version = version.as_str();
    format!(
        r#"set -euo pipefail
if [ "$(pi --version 2>/dev/null || true)" != "{version}" ]; then
  npm install -g --no-fund --no-audit "{PACKAGE}@{version}" >/dev/null
fi
"#
    )
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
