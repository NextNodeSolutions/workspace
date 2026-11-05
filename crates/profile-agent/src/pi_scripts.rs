use crate::options::Version;

const PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// `bin/provider-key.py` reads each provider key through the Infisical CLI,
/// signed in with the identity the secrets file exports.
pub(crate) const INFISICAL_INSTALL: &str = r#"set -euo pipefail
command -v infisical >/dev/null || npm install -g --no-fund --no-audit "@infisical/cli" >/dev/null
"#;

pub(crate) const OAUTH_KEY: &str = r#"set -euo pipefail
umask 077
dir="$HOME/.config/workspace"
file="$dir/secrets.env"
mkdir -p "$dir"
touch "$file"
if ! grep -q '^export PI_MCP_ADAPTER_OAUTH_FILE_KEY=' "$file"; then
  printf "export PI_MCP_ADAPTER_OAUTH_FILE_KEY='%s'\n" "$(head -c 32 /dev/urandom | base64 -w0)" >> "$file"
fi
"#;

/// Earlier versions of this profile mirrored Pi's own sign-in file.
pub(crate) const FORGET_SIGN_INS: &str = r#"set -euo pipefail
rm -f "$HOME/.pi/agent/mcp-auth.json"
"#;

/// Pi installs its `npm:` packages itself under `npm/`, so that directory
/// is left to it.
pub(crate) const DEPENDENCIES: &str = r#"set -euo pipefail
cd "$HOME/.pi/agent"
for dir in . packages/*/; do
  [ -f "$dir/pnpm-lock.yaml" ] || continue
  (cd "$dir" && pnpm install --frozen-lockfile --silent --config.strict-dep-builds=false)
done
"#;

/// The VM pulls the shared Codex access token from the vault now and every
/// hour, with the identity the secrets file exports. cron runs without the
/// shell's PATH, so the line carries node's directory.
pub(crate) const CODEX_PULL: &str = r#"set -euo pipefail
command -v crontab >/dev/null || { echo "cron is not installed on the VM, so the Codex token could not be kept fresh" >&2; exit 1; }
node_bin="$(dirname "$(command -v node)")"
line="7 * * * * export PATH=\"$node_bin:\$HOME/.local/bin:\$PATH\"; . \"\$HOME/.config/workspace/secrets.env\"; \"\$HOME/.local/bin/pi-token-pull\" >> \"\$HOME/.local/state/pi-token-pull.log\" 2>&1"
mkdir -p "$HOME/.local/state"
{ crontab -l 2>/dev/null | grep -vF 'pi-token-pull' || true; printf '%s\n' "$line"; } | crontab -
. "$HOME/.config/workspace/secrets.env"
"$HOME/.local/bin/pi-token-pull"
"#;

pub(crate) fn install(version: &Version) -> String {
    let version = version.as_str();
    format!(
        r#"set -euo pipefail
if [ "$(pi --version 2>/dev/null || true)" != "{version}" ]; then
  npm install -g --no-fund --no-audit "{PACKAGE}@{version}" >/dev/null
fi
"#
    )
}
