use std::path::Path;

use serde_json::Value;

use crate::error::AgentError;
use crate::json;

const OAUTH_TYPE: &str = "oauth";

/// Pi's auth file for the VM: this machine's API keys without its OAuth
/// sign-ins. The VM's own token pull adds the Codex access token.
pub(crate) fn auth_file(path: &Path) -> Result<Vec<u8>, AgentError> {
    json::rewrite(path, |fields| {
        fields.retain(|_, entry| entry.get("type").and_then(Value::as_str) != Some(OAUTH_TYPE));
    })
}
