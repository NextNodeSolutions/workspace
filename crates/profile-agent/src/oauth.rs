use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::options::{InvalidOption, SecretName, ServerName};

const SEPARATOR: char = '=';

const CODEX_TARGET: &str = "codex";

/// `codex=SECRET` fills Pi's Codex sign-in from a secret, and
/// `SERVER=SECRET` the sign-in of that MCP server.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct OauthSpec {
    pub(crate) target: Target,
    pub(crate) secret: SecretName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Codex,
    McpServer(ServerName),
}

impl FromStr for OauthSpec {
    type Err = InvalidOption;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (target, secret) = text
            .split_once(SEPARATOR)
            .ok_or_else(|| InvalidOption::OauthSpec(text.to_owned()))?;
        let target = if target == CODEX_TARGET {
            Target::Codex
        } else {
            Target::McpServer(target.parse()?)
        };
        Ok(Self {
            target,
            secret: secret.parse()?,
        })
    }
}

impl TryFrom<String> for OauthSpec {
    type Error = InvalidOption;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for OauthSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.target {
            Target::Codex => formatter.write_str(CODEX_TARGET)?,
            Target::McpServer(server) => write!(formatter, "{server}")?,
        }
        write!(formatter, "{SEPARATOR}{}", self.secret)
    }
}
