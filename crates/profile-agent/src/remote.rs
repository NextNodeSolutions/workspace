use std::fmt::Write;

use workspace_core::Cause;
use workspace_core::port::Shell;

use crate::infisical::Secret;

/// Prepended to `.bashrc`, before its early return, so that it also runs for
/// the non-interactive shells that ssh starts.
const HOOK_SCRIPT: &str = r#"set -euo pipefail
dir="$HOME/.config/workspace"
mkdir -p "$dir"
cat > "$dir/profile.sh.tmp" <<'PROFILE'
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
if [ -f "$HOME/.config/workspace/secrets.env" ]; then
  . "$HOME/.config/workspace/secrets.env"
fi
PROFILE
mv "$dir/profile.sh.tmp" "$dir/profile.sh"
line='[ -f "$HOME/.config/workspace/profile.sh" ] && . "$HOME/.config/workspace/profile.sh"'
touch "$HOME/.bashrc"
if ! grep -qxF "$line" "$HOME/.bashrc"; then
  { printf '%s\n' "$line"; cat "$HOME/.bashrc"; } > "$HOME/.bashrc.tmp"
  mv "$HOME/.bashrc.tmp" "$HOME/.bashrc"
fi
"#;

/// Reads `export NAME='value'` lines on stdin and replaces those names in
/// the secrets file, so that each profile owns its names and a rerun changes
/// nothing.
const SECRETS_SCRIPT: &str = r#"set -euo pipefail
umask 077
dir="$HOME/.config/workspace"
file="$dir/secrets.env"
mkdir -p "$dir"
new="$(cat)"
names="$(printf '%s\n' "$new" | sed -n 's/^export \([A-Za-z_][A-Za-z0-9_]*\)=.*/\1/p' | paste -sd'|' -)"
{
  if [ -f "$file" ] && [ -n "$names" ]; then
    grep -vE "^export ($names)=" "$file" || true
  fi
  printf '%s\n' "$new"
} > "$file.tmp"
mv "$file.tmp" "$file"
set -a
. "$file"
set +a
if [ -n "${GITHUB_TOKEN:-}" ] && command -v gh >/dev/null; then
  gh auth setup-git
fi
"#;

pub(crate) fn hook(shell: &dyn Shell) -> Result<(), Cause> {
    shell.run(HOOK_SCRIPT, &[]).map(drop)
}

pub(crate) fn extract(shell: &dyn Shell, directory: &str, archive: &[u8]) -> Result<(), Cause> {
    let script = format!(
        "set -euo pipefail\nmkdir -p \"$HOME/{directory}\"\ntar --warning=no-unknown-keyword -xzf - -C \"$HOME/{directory}\""
    );
    shell.run(&script, archive).map(drop)
}

pub(crate) fn write_file(shell: &dyn Shell, path: &str, bytes: &[u8]) -> Result<(), Cause> {
    let script = format!(
        "set -euo pipefail\nmkdir -p \"$(dirname \"$HOME/{path}\")\"\ncat > \"$HOME/{path}.tmp\"\nmv \"$HOME/{path}.tmp\" \"$HOME/{path}\""
    );
    shell.run(&script, bytes).map(drop)
}

pub(crate) fn export_secrets(shell: &dyn Shell, secrets: &[Secret]) -> Result<(), Cause> {
    let mut lines = String::new();
    for secret in secrets {
        let value = single_quoted(&secret.value);
        writeln!(lines, "export {}='{value}'", secret.name)
            .expect("writing to a String never fails");
    }
    shell.run(SECRETS_SCRIPT, lines.as_bytes()).map(drop)
}

fn single_quoted(value: &str) -> String {
    value.replace('\'', r"'\''")
}
