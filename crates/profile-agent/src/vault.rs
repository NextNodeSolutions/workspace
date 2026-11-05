use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::error::AgentError;
use crate::infisical::Secret;
use crate::local;
use crate::options::SecretName;

const INFISICAL: &str = "infisical";

const CURL: &str = "curl";

const SESSION_TIMEOUT: Duration = Duration::from_secs(30);

const CALL_TIMEOUT: Duration = Duration::from_secs(60);

const TOKEN_LINE_PREFIX: &str = "Token:";

const CLIENT_ID_VARIABLE: &str = "INFISICAL_UNIVERSAL_AUTH_CLIENT_ID";

const CLIENT_SECRET_VARIABLE: &str = "INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET";

const SECRET_DESCRIPTION: &str = "boats VM";

/// Long enough for a kept VM to pause between two starts; a start runs the
/// profile again and mints a new one.
const SECRET_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Deserialize)]
struct Binding {
    domain: String,
    clients: Clients,
}

#[derive(Deserialize)]
struct Clients {
    boats: Option<Identity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    identity_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UniversalAuth {
    identity_universal_auth: ClientId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientId {
    client_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Minted {
    client_secret: String,
}

/// A client secret of the vault's `boats` identity, the read-only one the
/// binding gives cloud VMs, minted for one VM with this machine's own Infisical session, as the two variables the
/// Infisical CLI and the token pull read on the VM.
pub(crate) fn mint(binding_path: &Path) -> Result<Vec<Secret>, AgentError> {
    let binding = binding(binding_path)?;
    let identity = binding
        .clients
        .boats
        .ok_or_else(|| AgentError::NoVaultIdentity(binding_path.to_owned()))?;
    let session = session_token()?;
    let base = format!(
        "{}/v1/auth/universal-auth/identities/{}",
        binding.domain.trim_end_matches('/'),
        identity.identity_id
    );
    let auth: UniversalAuth = call(&session, "GET", &base, None)?;
    let body = format!(
        r#"{{"description":"{SECRET_DESCRIPTION}","ttl":{SECRET_TTL_SECONDS},"numUsesLimit":0}}"#
    );
    let minted: Minted = call(
        &session,
        "POST",
        &format!("{base}/client-secrets"),
        Some(&body),
    )?;
    Ok(vec![
        secret(CLIENT_ID_VARIABLE, auth.identity_universal_auth.client_id),
        secret(CLIENT_SECRET_VARIABLE, minted.client_secret),
    ])
}

fn binding(path: &Path) -> Result<Binding, AgentError> {
    let text = fs::read(path).map_err(|source| AgentError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&text).map_err(|_| AgentError::NoVaultIdentity(path.to_owned()))
}

/// Exactly one `Token:` line with a single word after it; anything else
/// means no session, and is never shown.
fn session_token() -> Result<String, AgentError> {
    let mut command = Command::new(INFISICAL);
    command.args(["user", "get", "token"]);
    let stdout =
        local::run(INFISICAL, command, SESSION_TIMEOUT).map_err(|_| AgentError::NoVaultSession)?;
    let text = String::from_utf8_lossy(&stdout);
    let mut tokens = text
        .lines()
        .filter_map(|line| line.strip_prefix(TOKEN_LINE_PREFIX))
        .map(str::trim);
    match (tokens.next(), tokens.next()) {
        (Some(token), None) if !token.is_empty() && !token.contains(char::is_whitespace) => {
            Ok(token.to_owned())
        }
        _ => Err(AgentError::NoVaultSession),
    }
}

/// The session token reaches curl as a config file on stdin, never as an
/// argument. The body holds no secret, so it may be one.
fn call<T: for<'de> Deserialize<'de>>(
    session: &str,
    method: &str,
    url: &str,
    body: Option<&str>,
) -> Result<T, AgentError> {
    let mut command = Command::new(CURL);
    command.args(["--silent", "--show-error", "--fail", "--config", "-"]);
    command.args(["--request", method, url]);
    command.args(["--header", "content-type: application/json"]);
    if let Some(body) = body {
        command.args(["--data", body]);
    }
    let config = format!("header = \"Authorization: Bearer {session}\"\n");
    let stdout = local::run_with_input(CURL, command, config.as_bytes(), CALL_TIMEOUT)
        .map_err(|_| AgentError::VaultRefused)?;
    serde_json::from_slice(&stdout).map_err(|_| AgentError::VaultRefused)
}

fn secret(name: &str, value: String) -> Secret {
    Secret {
        name: name
            .parse::<SecretName>()
            .expect("the variable names are valid secret names"),
        value,
    }
}
