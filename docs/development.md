# Development

## Checks

Every commit must pass these with no warning:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The workspace turns on pedantic clippy lints and forbids `unsafe`.

## Conventions

| Rule | Enforced by |
| --- | --- |
| A function stays short and simple. | clippy, with the limits in `clippy.toml` |
| A file stays under 250 lines. | review |
| Nesting goes no deeper than 2 levels. | review |
| Every literal gets a named constant. No magic numbers. | review |
| A commit holds one change, follows [Conventional Commits](https://www.conventionalcommits.org), and passes the checks on its own. | review |

Code, comments, commits and docs are in English.

## Adding a provider or a profile

1. Create a crate that depends only on the core, and implement `Provider` or `Profile`.
2. Add it as a dependency of the cli crate.
3. Register it by name in the cli's adapter registry.
4. If it takes options, give it one options struct that derives clap's `Args` and serde's `Deserialize`, and add that struct to the cli's provider options. That gives the adapter both its flags and its `[providers.<name>]` table.

The core does not change. The Boat adapter is a working example of all four steps.
