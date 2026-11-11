use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::AgentError;

/// Reads the JSON object at `path`, lets `edit` change it, and returns the
/// result pretty-printed. A missing file is an empty object.
pub(crate) fn rewrite(
    path: &Path,
    edit: impl FnOnce(&mut Map<String, Value>),
) -> Result<Vec<u8>, AgentError> {
    let mut fields = match fs::read(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Map::new(),
        text => parse(
            path,
            &text.map_err(|source| AgentError::Read {
                path: path.to_owned(),
                source,
            })?,
        )?,
    };
    edit(&mut fields);
    Ok(serde_json::to_vec_pretty(&Value::Object(fields)).expect("a JSON value serializes"))
}

fn parse(path: &Path, text: &[u8]) -> Result<Map<String, Value>, AgentError> {
    match serde_json::from_slice(text) {
        Ok(Value::Object(fields)) => Ok(fields),
        _ => Err(AgentError::NotObject(path.to_owned())),
    }
}
