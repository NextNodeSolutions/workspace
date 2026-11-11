use std::time::Duration;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

/// Largest unit first, so `show` picks the shortest spelling.
const UNITS: [(&str, u64); 4] = [
    ("d", SECONDS_PER_DAY),
    ("h", SECONDS_PER_HOUR),
    ("m", SECONDS_PER_MINUTE),
    ("s", 1),
];

/// A count and a unit, such as `8h`, `90m` or `30s`; zero is no duration.
#[must_use]
pub fn parse(text: &str) -> Option<Duration> {
    let digits = text.find(|c: char| !c.is_ascii_digit())?;
    let (count, suffix) = text.split_at(digits);
    let (_, unit) = UNITS.iter().find(|(name, _)| *name == suffix)?;
    let count: u64 = count.parse().ok()?;
    let seconds = count.checked_mul(*unit).filter(|&seconds| seconds > 0)?;
    Some(Duration::from_secs(seconds))
}

/// The spelling `parse` reads back, in the largest unit that divides it.
#[must_use]
pub fn show(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (name, unit) = UNITS
        .iter()
        .find(|(_, unit)| seconds.is_multiple_of(*unit))
        .unwrap_or(&UNITS[UNITS.len() - 1]);
    format!("{}{name}", seconds / unit)
}
