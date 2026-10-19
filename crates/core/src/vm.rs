use std::fmt;

use crate::Error;

/// Longest DNS label, so every provider can use the name as a hostname.
pub(crate) const MAX_NAME_LENGTH: usize = 63;

#[derive(Debug, Clone)]
pub struct VmSpec {
    name: String,
}

impl VmSpec {
    /// # Errors
    ///
    /// [`Error::InvalidName`] unless `name` is a DNS label.
    pub fn new(name: &str) -> Result<Self, Error> {
        if !is_dns_label(name) {
            return Err(Error::InvalidName {
                name: name.to_owned(),
            });
        }
        Ok(Self {
            name: name.to_owned(),
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn is_dns_label(name: &str) -> bool {
    let has_valid_characters = name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let has_valid_length = !name.is_empty() && name.len() <= MAX_NAME_LENGTH;
    has_valid_length && has_valid_characters && !name.starts_with('-') && !name.ends_with('-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmId(String);

impl VmId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for VmId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for VmId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
