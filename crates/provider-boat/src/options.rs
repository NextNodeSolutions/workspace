use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use clap::{Args, ValueEnum};
use serde::Deserialize;
use workspace_core::duration;

const SECONDS_PER_HOUR: u64 = 60 * 60;

const DEFAULT_MACHINE_TYPE: MachineType = MachineType::Small;

const DEFAULT_TTL: Ttl = Ttl(8 * SECONDS_PER_HOUR);

const DEFAULT_ENVIRONMENT: &str = "main";

#[derive(Args, Clone, Default, Deserialize)]
#[command(next_help_heading = "Boat")]
#[serde(deny_unknown_fields)]
pub struct Options {
    /// Machine size [default: small]
    #[arg(long = "boat-type", value_name = "TYPE")]
    #[serde(rename = "type")]
    machine_type: Option<MachineType>,

    /// How long after it starts Boat stops the sandbox, even mid-work, such as
    /// 8h or 90m [default: 8h]
    #[arg(long = "boat-ttl", value_name = "DURATION")]
    ttl: Option<Ttl>,

    /// Boat environment the sandboxes start from, which setup prepares
    /// [default: main]
    #[arg(long = "boat-environment", value_name = "NAME")]
    environment: Option<BoatName>,

    /// Saved snapshot that new sandboxes start from, as `workspace snapshot`
    /// saves it [default: Boat's base image]
    #[arg(long = "boat-from", value_name = "NAME")]
    from: Option<BoatName>,
}

impl Options {
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            machine_type: self.machine_type.or(fallback.machine_type),
            ttl: self.ttl.or(fallback.ttl),
            environment: self.environment.or(fallback.environment),
            from: self.from.or(fallback.from),
        }
    }

    pub(crate) fn resolve(self) -> Settings {
        Settings {
            machine_type: self.machine_type.unwrap_or(DEFAULT_MACHINE_TYPE),
            ttl: self.ttl.unwrap_or(DEFAULT_TTL),
            environment: self
                .environment
                .unwrap_or_else(|| BoatName(DEFAULT_ENVIRONMENT.to_owned())),
            from: self.from,
        }
    }
}

pub(crate) struct Settings {
    pub(crate) machine_type: MachineType,
    pub(crate) ttl: Ttl,
    pub(crate) environment: BoatName,
    pub(crate) from: Option<BoatName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MachineType {
    /// 2 vCPU, 4 GB, at half the default rate
    Small,
    /// 4 vCPU, 8 GB
    Default,
    /// 8 vCPU, 16 GB, at twice the default rate
    Large,
    /// 16 vCPU, 32 GB, on the $100/month plan or higher
    Xlarge,
}

impl MachineType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Default => "default",
            Self::Large => "large",
            Self::Xlarge => "xlarge",
        }
    }
}

impl fmt::Display for MachineType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct Ttl(u64);

impl Ttl {
    pub(crate) fn seconds(self) -> u64 {
        self.0
    }
}

impl FromStr for Ttl {
    type Err = InvalidOption;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        duration::parse(text)
            .map(|duration| Self(duration.as_secs()))
            .ok_or_else(|| InvalidOption::Ttl(text.to_owned()))
    }
}

impl TryFrom<String> for Ttl {
    type Error = InvalidOption;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for Ttl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&duration::show(Duration::from_secs(self.0)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct BoatName(String);

impl BoatName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for BoatName {
    type Err = InvalidOption;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
        if name.is_empty() || name.starts_with('-') || !name.chars().all(is_allowed) {
            return Err(InvalidOption::BoatName(name.to_owned()));
        }
        Ok(Self(name.to_owned()))
    }
}

impl TryFrom<String> for BoatName {
    type Error = InvalidOption;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        name.parse()
    }
}

impl fmt::Display for BoatName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidOption {
    #[error("`{0}` is not a duration: write a positive whole number then d, h, m or s, such as 8h")]
    Ttl(String),

    #[error(
        "`{0}` cannot name a Boat environment or snapshot: use ASCII letters, digits, dots, dashes and underscores, and do not start with a dash"
    )]
    BoatName(String),
}
