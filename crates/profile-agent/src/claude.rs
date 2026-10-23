use std::fs;
use std::io;
use std::path::Path;

use clap::Args;
use serde::Deserialize;
use serde_json::Value;
use workspace_core::Cause;
use workspace_core::port::{Profile, Shell};

use crate::error::AgentError;
use crate::options::{Identifier, SecretName, SecretSettings};
use crate::{archive, infisical, local, remote};

const NAME: &str = "claude";

const DIRECTORY: &str = ".claude";

const SETTINGS: &str = "settings.json";

const DEFAULT_SECRETS: &[&str] = &["CLAUDE_CODE_OAUTH_TOKEN", "GITHUB_TOKEN"];

const MIRRORED: &[&str] = &["CLAUDE.md", "agents", "commands", "skills"];

/// These run programs of this machine, which the VM does not have.
const LOCAL_ONLY_SETTINGS: &[&str] = &["hooks", "statusLine"];

const INSTALL_SCRIPT: &str = r#"set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
if ! command -v claude >/dev/null; then
  curl -fsSL https://claude.ai/install.sh | bash >/dev/null
fi
"#;

#[derive(Args, Clone, Default, Deserialize)]
#[command(next_help_heading = "Claude Code profile")]
#[serde(deny_unknown_fields)]
pub struct ClaudeOptions {
    /// Infisical project the secrets come from
    #[arg(long = "claude-infisical-project", value_name = "ID")]
    infisical_project: Option<Identifier>,

    /// Infisical environment the secrets come from [default: dev]
    #[arg(long = "claude-infisical-env", value_name = "NAME")]
    infisical_env: Option<Identifier>,

    /// Secrets to export in every shell on the VM, comma-separated
    /// [default: `CLAUDE_CODE_OAUTH_TOKEN,GITHUB_TOKEN`]
    #[arg(long = "claude-secrets", value_name = "NAMES", value_delimiter = ',')]
    secrets: Option<Vec<SecretName>>,
}

impl ClaudeOptions {
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            infisical_project: self.infisical_project.or(fallback.infisical_project),
            infisical_env: self.infisical_env.or(fallback.infisical_env),
            secrets: self.secrets.or(fallback.secrets),
        }
    }
}

pub struct Claude {
    secrets: SecretSettings,
}

impl Claude {
    #[must_use]
    pub fn new(options: ClaudeOptions) -> Self {
        Self {
            secrets: SecretSettings::new(
                options.infisical_project,
                options.infisical_env,
                options.secrets,
                DEFAULT_SECRETS,
            ),
        }
    }
}

impl Profile for Claude {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Everything local is gathered first, so a missing secret or file fails
    /// before the VM changes.
    fn provision(&self, shell: &dyn Shell) -> Result<(), Cause> {
        let secrets = match self.secrets.source(NAME)? {
            Some(source) => infisical::fetch(&source)?,
            None => Vec::new(),
        };
        let directory = local::home()?.join(DIRECTORY);
        let archive = archive::pack(&directory, MIRRORED)?;
        let settings = portable_settings(&directory.join(SETTINGS))?;
        remote::hook(shell)?;
        shell.run(INSTALL_SCRIPT, &[])?;
        remote::extract(shell, DIRECTORY, &archive)?;
        if let Some(settings) = settings {
            remote::write_file(shell, &format!("{DIRECTORY}/{SETTINGS}"), &settings)?;
        }
        if !secrets.is_empty() {
            remote::export_secrets(shell, &secrets)?;
        }
        Ok(())
    }
}

fn portable_settings(path: &Path) -> Result<Option<Vec<u8>>, AgentError> {
    let text = match fs::read(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        text => text.map_err(|source| AgentError::Read {
            path: path.to_owned(),
            source,
        })?,
    };
    let mut settings: Value =
        serde_json::from_slice(&text).map_err(|_| AgentError::NotSettings(path.to_owned()))?;
    let fields = settings
        .as_object_mut()
        .ok_or_else(|| AgentError::NotSettings(path.to_owned()))?;
    for key in LOCAL_ONLY_SETTINGS {
        fields.remove(*key);
    }
    let text = serde_json::to_vec_pretty(&settings).expect("a JSON value serializes");
    Ok(Some(text))
}
