use std::fmt;

use crate::Error;

/// Longest DNS label, so every provider can use the name as a hostname.
pub(crate) const MAX_NAME_LENGTH: usize = 63;

#[derive(Debug, Clone)]
pub struct VmSpec {
    name: Option<String>,
}

impl VmSpec {
    /// A named VM is kept: `start` resumes it and `stop` keeps its disk.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidName`] unless `name` is a DNS label.
    pub fn named(name: &str) -> Result<Self, Error> {
        if !is_dns_label(name) {
            return Err(Error::InvalidName {
                name: name.to_owned(),
            });
        }
        Ok(Self {
            name: Some(name.to_owned()),
        })
    }

    /// A VM without a name is disposable: it is deleted once you leave it,
    /// and a stop deletes it too.
    #[must_use]
    pub fn disposable() -> Self {
        Self { name: None }
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub fn is_disposable(&self) -> bool {
        self.name.is_none()
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
