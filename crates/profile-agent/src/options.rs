use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

macro_rules! checked_string {
    ($name:ident, $is_valid:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
        #[serde(try_from = "String")]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = InvalidOption;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                if !$is_valid(text) {
                    return Err(InvalidOption::$name(text.to_owned()));
                }
                Ok(Self(text.to_owned()))
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidOption;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                text.parse()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

checked_string!(SecretName, is_secret_name);
checked_string!(Identifier, is_identifier);
checked_string!(Version, is_version);

fn is_secret_name(text: &str) -> bool {
    let mut chars = text.chars();
    let starts_well = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    starts_well && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_identifier(text: &str) -> bool {
    let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    !text.is_empty() && !text.starts_with('-') && text.chars().all(is_allowed)
}

fn is_version(text: &str) -> bool {
    let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+');
    !text.is_empty() && !text.starts_with('-') && text.chars().all(is_allowed)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidOption {
    #[error(
        "`{0}` cannot name a secret: use ASCII letters, digits and underscores, and do not start with a digit"
    )]
    SecretName(String),

    #[error(
        "`{0}` cannot name an Infisical project or environment: use ASCII letters, digits, dots, dashes and underscores, and do not start with a dash"
    )]
    Identifier(String),

    #[error(
        "`{0}` is not a version: use ASCII letters, digits, dots, dashes and pluses, and do not start with a dash"
    )]
    Version(String),
}
