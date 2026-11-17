use std::error::Error;

#[derive(Default)]
pub(super) struct Environment {
    entries: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum EnvironmentError {
    #[error("invalid environment name: use shell variable names with --env NAME")]
    InvalidName,
    #[error("cannot read --env {0}: export a UTF-8 value in the caller's environment")]
    Unavailable(String),
    #[error(
        "a selected runtime value appears in the job's script, options or --push files; pass it only through --env NAME"
    )]
    InlineValue,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct MaskedError {
    message: String,
    #[source]
    source: Option<Box<Self>>,
}

impl Environment {
    pub(super) fn inherit(names: &[String]) -> Result<Self, EnvironmentError> {
        let mut entries = names
            .iter()
            .map(|name| {
                validate_name(name)?;
                let value =
                    std::env::var(name).map_err(|_| EnvironmentError::Unavailable(name.clone()))?;
                Ok((name.clone(), value))
            })
            .collect::<Result<Vec<_>, EnvironmentError>>()?;
        entries.sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
        Ok(Self { entries })
    }

    pub(super) fn stdin(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (name, value) in &self.entries {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(b'=');
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    pub(super) fn reject_inline(&self, script: &str) -> Result<(), EnvironmentError> {
        self.reject_bytes(script.as_bytes())
    }

    pub(super) fn reject_bytes(&self, bytes: &[u8]) -> Result<(), EnvironmentError> {
        if self.entries.iter().any(|(_, value)| {
            !value.is_empty()
                && bytes
                    .windows(value.len())
                    .any(|window| window == value.as_bytes())
        }) {
            return Err(EnvironmentError::InlineValue);
        }
        Ok(())
    }

    pub(super) fn mask(&self, bytes: &[u8]) -> Vec<u8> {
        self.entries
            .iter()
            .fold(bytes.to_vec(), |bytes, (_, value)| {
                replace(&bytes, value.as_bytes())
            })
    }

    pub(super) fn message(&self, message: &str) -> String {
        String::from_utf8_lossy(&self.mask(message.as_bytes())).into_owned()
    }

    pub(super) fn mask_truncated(&self, bytes: &[u8]) -> Vec<u8> {
        let mut masked = self.mask(bytes);
        let overlap = self
            .entries
            .iter()
            .map(|(_, value)| value.len().saturating_sub(1))
            .max()
            .unwrap_or_default();
        masked.truncate(masked.len().saturating_sub(overlap));
        masked
    }

    pub(super) fn error(&self, error: &dyn Error) -> Box<dyn Error> {
        let messages = std::iter::successors(Some(error), |&cause| cause.source())
            .map(|cause| self.message(&cause.to_string()))
            .collect::<Vec<_>>();
        let mut source = None;
        for message in messages.into_iter().rev() {
            source = Some(Box::new(MaskedError { message, source }));
        }
        source.expect("the error chain contains the root error")
    }
}

fn validate_name(name: &str) -> Result<(), EnvironmentError> {
    let mut characters = name.chars();
    let is_first_valid = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    if !is_first_valid
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(EnvironmentError::InvalidName);
    }
    Ok(())
}

fn replace(bytes: &[u8], value: &[u8]) -> Vec<u8> {
    const MASK: &[u8] = b"***";
    if value.is_empty() {
        return bytes.to_vec();
    }
    let mut remaining = bytes;
    let mut masked = Vec::new();
    while let Some(position) = remaining
        .windows(value.len())
        .position(|window| window == value)
    {
        masked.extend_from_slice(&remaining[..position]);
        masked.extend_from_slice(MASK);
        remaining = &remaining[position + value.len()..];
    }
    masked.extend_from_slice(remaining);
    masked
}
