use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::error::AgentError;
use crate::infisical::Secret;
use crate::json;
use crate::oauth::{OauthSpec, Target};

const OAUTH_TYPE: &str = "oauth";

const CODEX_PROVIDER: &str = "openai-codex";

/// A refresh token is exclusive to this machine: a refresh from a VM would
/// end the sessions of this machine and of every other VM.
const PLACEHOLDER_REFRESH: &str = "workspace-placeholder";

/// Reads a JSON list of `{name, entry}` on stdin and saves each sign-in in
/// the adapter's encrypted store, through the adapter's own code.
pub(crate) const MCP_SIGN_IN_SCRIPT: &str = r#"set -euo pipefail
. "$HOME/.config/workspace/secrets.env"
cd "$HOME/.pi/agent/npm"
node --input-type=module -e '
import { readFileSync } from "node:fs";
import { saveAuthEntry } from "./node_modules/pi-mcp-adapter/dist/mcp-auth.js";
const options = { credentialStore: "encrypted-file" };
for (const { name, entry } of JSON.parse(readFileSync(0, "utf8"))) {
  saveAuthEntry(name, entry, entry.serverUrl, options);
}
'
"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexToken {
    access: String,
    expires: u64,
    account_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpSignIn {
    server_url: String,
    tokens: McpTokens,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpTokens {
    access_token: String,
    expires_at: Option<f64>,
    scope: Option<String>,
    issuer: Option<String>,
}

/// Pi's auth file for the VM: this machine's API keys without its OAuth
/// sign-ins, plus the Codex access token when the profile carries one.
pub(crate) fn auth_file(
    path: &Path,
    specs: &[OauthSpec],
    secrets: &[Secret],
) -> Result<Vec<u8>, AgentError> {
    let codex = specs
        .iter()
        .find(|spec| spec.target == Target::Codex)
        .map(|spec| codex_entry(secret_named(secrets, spec)))
        .transpose()?;
    json::rewrite(path, |fields| {
        fields.retain(|_, entry| entry.get("type").and_then(Value::as_str) != Some(OAUTH_TYPE));
        if let Some(entry) = codex {
            fields.insert(CODEX_PROVIDER.to_owned(), entry);
        }
    })
}

/// The MCP sign-ins to save on the VM, as the sign-in script reads them, or
/// nothing when the profile carries none.
pub(crate) fn mcp_sign_ins(
    specs: &[OauthSpec],
    secrets: &[Secret],
) -> Result<Option<Vec<u8>>, AgentError> {
    let mut entries = Vec::new();
    for spec in specs {
        let Target::McpServer(server) = &spec.target else {
            continue;
        };
        let sign_in: McpSignIn = parse(secret_named(secrets, spec))?;
        entries.push(json!({
            "name": server.as_str(),
            "entry": { "serverUrl": sign_in.server_url, "tokens": tokens_json(sign_in.tokens) },
        }));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::to_vec(&entries).expect("a JSON value serializes"),
    ))
}

fn tokens_json(tokens: McpTokens) -> Value {
    let mut fields = Map::new();
    fields.insert("accessToken".to_owned(), Value::String(tokens.access_token));
    if let Some(expires_at) = tokens.expires_at {
        fields.insert("expiresAt".to_owned(), json!(expires_at));
    }
    if let Some(scope) = tokens.scope {
        fields.insert("scope".to_owned(), Value::String(scope));
    }
    if let Some(issuer) = tokens.issuer {
        fields.insert("issuer".to_owned(), Value::String(issuer));
    }
    Value::Object(fields)
}

fn codex_entry(secret: &Secret) -> Result<Value, AgentError> {
    let token: CodexToken = parse(secret)?;
    Ok(json!({
        "type": OAUTH_TYPE,
        "access": token.access,
        "refresh": PLACEHOLDER_REFRESH,
        "expires": token.expires,
        "accountId": token.account_id,
    }))
}

fn parse<T: for<'de> Deserialize<'de>>(secret: &Secret) -> Result<T, AgentError> {
    serde_json::from_str(&secret.value).map_err(|_| AgentError::NotASignIn {
        name: secret.name.to_string(),
    })
}

fn secret_named<'a>(secrets: &'a [Secret], spec: &OauthSpec) -> &'a Secret {
    secrets
        .iter()
        .find(|secret| secret.name == spec.secret)
        .expect("every sign-in secret was fetched")
}
