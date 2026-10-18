//! What a user can set for Boat, on the command line or in the config file,
//! and the defaults for what they leave unset.

use std::fmt;
use std::str::FromStr;

use clap::{Args, ValueEnum};
use serde::Deserialize;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

/// The suffixes a [`Ttl`] takes, largest first, with their length in seconds.
const UNITS: [(&str, u64); 4] = [
    ("d", SECONDS_PER_DAY),
    ("h", SECONDS_PER_HOUR),
    ("m", SECONDS_PER_MINUTE),
    ("s", 1),
];

/// The operator's sizing rule: `small`, unless someone asks for more.
const DEFAULT_MACHINE_TYPE: MachineType = MachineType::Small;

/// Boat's timer cannot tell when an agent goes idle, so this is a hard
/// limit. Stopping an idle sandbox sooner is up to the profile.
const DEFAULT_TTL: Ttl = Ttl(8 * SECONDS_PER_HOUR);

/// The environment onboarding creates for workspace's sandboxes.
const DEFAULT_ENVIRONMENT: &str = "workspace";

/// Boat's options. Each one left unset falls back to the config file, then
/// to its default.
#[derive(Args, Clone, Default, Deserialize)]
#[command(next_help_heading = "Boat")]
#[serde(deny_unknown_fields)]
pub struct Options {
    /// Machine size [default: small]
    #[arg(long = "boat-type", value_name = "TYPE")]
    #[serde(rename = "type")]
    machine_type: Option<MachineType>,

    /// How long after boot Boat stops the sandbox, even mid-work, such as
    /// 8h or 90m [default: 8h]
    #[arg(long = "boat-ttl", value_name = "DURATION")]
    ttl: Option<Ttl>,

    /// Boat environment the sandboxes start from, which onboard sets up
    /// [default: workspace]
    #[arg(long = "boat-environment", value_name = "NAME")]
    environment: Option<EnvironmentName>,
}

impl Options {
    /// These options, with each unset one taken from `fallback`.
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            machine_type: self.machine_type.or(fallback.machine_type),
            ttl: self.ttl.or(fallback.ttl),
            environment: self.environment.or(fallback.environment),
        }
    }

    pub(crate) fn resolve(self) -> Settings {
        Settings {
            machine_type: self.machine_type.unwrap_or(DEFAULT_MACHINE_TYPE),
            ttl: self.ttl.unwrap_or(DEFAULT_TTL),
            environment: self
                .environment
                .unwrap_or_else(|| EnvironmentName(DEFAULT_ENVIRONMENT.to_owned())),
        }
    }
}

/// The options with every default filled in.
pub(crate) struct Settings {
    pub(crate) machine_type: MachineType,
    pub(crate) ttl: Ttl,
    pub(crate) environment: EnvironmentName,
}

/// A Boat machine size, named as `boat new --type` takes it.
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

/// A sandbox lifetime: a whole number of days, hours, minutes or seconds,
/// such as `8h`.
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
        let invalid = || InvalidOption::Ttl(text.to_owned());
        let digits = text
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(invalid)?;
        let (count, suffix) = text.split_at(digits);
        let (_, unit) = UNITS
            .iter()
            .find(|(name, _)| *name == suffix)
            .ok_or_else(invalid)?;
        let count: u64 = count.parse().map_err(|_| invalid())?;
        let seconds = count.checked_mul(*unit).filter(|&seconds| seconds > 0);
        seconds.map(Self).ok_or_else(invalid)
    }
}

impl TryFrom<String> for Ttl {
    type Error = InvalidOption;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

/// Written in the largest unit that divides it, so `8h` reads back as `8h`.
impl fmt::Display for Ttl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, unit) = UNITS
            .iter()
            .find(|(_, unit)| self.0 % unit == 0)
            .expect("every count of seconds divides by one second");
        write!(formatter, "{}{name}", self.0 / unit)
    }
}

/// The name of a Boat environment: ASCII letters, digits, dots, dashes and
/// underscores, as Boat allows. It cannot start with a dash, which `boat`
/// would read as a flag.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct EnvironmentName(String);

impl EnvironmentName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for EnvironmentName {
    type Err = InvalidOption;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
        if name.is_empty() || name.starts_with('-') || !name.chars().all(is_allowed) {
            return Err(InvalidOption::EnvironmentName(name.to_owned()));
        }
        Ok(Self(name.to_owned()))
    }
}

impl TryFrom<String> for EnvironmentName {
    type Error = InvalidOption;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        name.parse()
    }
}

impl fmt::Display for EnvironmentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A value that cannot be a Boat option.
#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidOption {
    #[error("`{0}` is not a duration: write a positive whole number then d, h, m or s, such as 8h")]
    Ttl(String),

    #[error(
        "`{0}` cannot name a Boat environment: use ASCII letters, digits, dots, dashes and underscores, and do not start with a dash"
    )]
    EnvironmentName(String),
}
