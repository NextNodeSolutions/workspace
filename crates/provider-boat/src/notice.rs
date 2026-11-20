use std::fmt;
use std::process::ExitStatus;

use serde::Deserialize;

const SANDBOX_PREFIX: &str = "bx_";
const MAX_ID_BYTES: usize = 64;
const UNRECOGNIZED_ID: &str = "[unrecognized id]";
const HTTP_ERROR_STATUSES: std::ops::RangeInclusive<u16> = 400..=599;

#[derive(Deserialize)]
pub(crate) struct Refusal {
    code: Option<Code>,
    status: Option<u16>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Code {
    ApiKeyActionForbidden,
    #[serde(other)]
    Unknown,
}

/// A diagnostic for a Boat create command that already returned a sandbox id.
pub struct Notice {
    operation: &'static str,
    id: String,
    cause: Cause,
}

enum Cause {
    Refused(Refusal),
    Ended(ExitStatus),
    Unconfirmed,
}

/// Receives diagnostics that do not change the result of a create.
pub trait Warnings: Send + Sync {
    fn warning(&self, notice: Notice);
}

impl Notice {
    pub(crate) fn refused(operation: &'static str, id: &str, refusal: Refusal) -> Self {
        Self::new(operation, id, Cause::Refused(refusal))
    }

    pub(crate) fn ended(operation: &'static str, id: &str, status: ExitStatus) -> Self {
        Self::new(operation, id, Cause::Ended(status))
    }

    pub(crate) fn unconfirmed(operation: &'static str, id: &str) -> Self {
        Self::new(operation, id, Cause::Unconfirmed)
    }

    fn new(operation: &'static str, id: &str, cause: Cause) -> Self {
        let id = if id.len() <= MAX_ID_BYTES
            && id.strip_prefix(SANDBOX_PREFIX).is_some_and(valid_suffix)
        {
            id
        } else {
            UNRECOGNIZED_ID
        };
        Self {
            operation,
            id: id.to_owned(),
            cause,
        }
    }
}

fn valid_suffix(suffix: &str) -> bool {
    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

impl fmt::Display for Notice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`boat {}` reported a problem after assigning sandbox `{}`: ",
            self.operation, self.id
        )?;
        match &self.cause {
            Cause::Refused(refusal) => refusal.fmt(formatter),
            Cause::Ended(status) => {
                write!(
                    formatter,
                    "the CLI ended with {status} without an error event"
                )
            }
            Cause::Unconfirmed => formatter.write_str("could not confirm the CLI result"),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(Code::ApiKeyActionForbidden) => {
                formatter.write_str("code api_key_action_forbidden")?;
            }
            Some(Code::Unknown) | None => formatter.write_str("an error without a code")?,
        }
        if let Some(status) = self
            .status
            .filter(|status| HTTP_ERROR_STATUSES.contains(status))
        {
            write!(formatter, ", HTTP {status}")?;
        }
        Ok(())
    }
}
