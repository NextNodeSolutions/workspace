use crate::options::Version;

const PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// A VM gets the agent directory from the public repository, never from this
/// machine. A directory without Git, such as one Pi made at its first start,
/// is adopted in place: the repository's files replace those it tracks, the
/// leftovers in its tracked folders go, and what it ignores (sessions,
/// sign-ins, installed dependencies) stays. A checkout only fast-forwards,
/// so work done on the VM is never overwritten.
pub(crate) const CHECKOUT: &str = r#"set -euo pipefail
repository="https://github.com/NextNodeSolutions/pi-agent.git"
dir="$HOME/.pi/agent"
if [ -d "$dir/.git" ]; then
  git -C "$dir" pull --ff-only --quiet || {
    echo "~/.pi/agent on the VM has changes that block the update from $repository: commit, stash or drop them on the VM, then run the profile again" >&2
    exit 1
  }
  exit 0
fi
mkdir -p "$dir"
clone="$(mktemp -d "$HOME/.pi/agent-clone.XXXXXX")"
trap 'rm -rf "$clone"' EXIT
git clone --quiet --no-checkout "$repository" "$clone"
mv "$clone/.git" "$dir/.git"
git -C "$dir" reset --quiet --hard
git -C "$dir" clean -fd --quiet
"#;

/// `bin/setup.ts` pins the shared settings and installs the locked
/// dependencies; Pi installs its `npm:` packages itself.
pub(crate) const SETUP: &str = r#"set -euo pipefail
cd "$HOME/.pi/agent"
node bin/setup.ts
"#;

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

/// The VM pulls the shared Codex access token from the vault now and every
/// hour, with the identity the secrets file exports, through the program the
/// agent repository holds. cron runs without the shell's PATH, so the line
/// carries node's directory.
pub(crate) const CODEX_PULL: &str = r#"set -euo pipefail
command -v crontab >/dev/null || { echo "cron is not installed on the VM, so the Codex token could not be kept fresh" >&2; exit 1; }
node_bin="$(dirname "$(command -v node)")"
line="7 * * * * export PATH=\"$node_bin:\$HOME/.local/bin:\$PATH\"; . \"\$HOME/.config/workspace/secrets.env\"; \"\$HOME/.pi/agent/bin/pi-token-pull\" >> \"\$HOME/.local/state/pi-token-pull.log\" 2>&1"
mkdir -p "$HOME/.local/state"
{ crontab -l 2>/dev/null | grep -vF 'pi-token-pull' || true; printf '%s\n' "$line"; } | crontab -
. "$HOME/.config/workspace/secrets.env"
"$HOME/.pi/agent/bin/pi-token-pull"
"#;

/// The join key on stdin works once and no copy of it is kept; a VM already on
/// the tailnet leaves it unused. The checkout's `tailnet-link setup` joins
/// when needed, then serves the VM's links.
pub(crate) const TAILNET: &str = r#"set -euo pipefail
TS_AUTHKEY="$(cat)"
export TS_AUTHKEY
"$HOME/.pi/agent/skills/tailnet-links/scripts/tailnet-link" setup
"#;

/// Without a version option, the VM installs the version the checkout's
/// `package.json` pins, since the extensions were written against it.
pub(crate) fn install(version: Option<&Version>) -> String {
    let pick = match version {
        Some(version) => format!("version=\"{}\"", version.as_str()),
        None => format!(
            r#"version="$(node -p 'require(process.env.HOME + "/.pi/agent/package.json").devDependencies["{PACKAGE}"] ?? ""')"
case "$version" in
  ''|-*|*[!A-Za-z0-9.+-]*) echo "~/.pi/agent/package.json pins no exact {PACKAGE} version (found '$version'): pass --pi-version" >&2; exit 1 ;;
esac"#
        ),
    };
    format!(
        r#"set -euo pipefail
{pick}
if [ "$(pi --version 2>/dev/null || true)" != "$version" ]; then
  npm install -g --no-fund --no-audit "{PACKAGE}@$version" >/dev/null
fi
"#
    )
}
