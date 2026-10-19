# Development

## Checks

Run these before every commit. All must pass with no warning.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The workspace turns on `clippy::pedantic` and forbids `unsafe`. There are no automated tests yet.

## Conventions

| Rule | Limit |
| --- | --- |
| Function length | 50 lines, checked by clippy through `clippy.toml` |
| Cognitive complexity | 15, checked by clippy |
| File length | 250 lines |
| Nesting | 2 levels |
| Literals | Named constants, no magic numbers |

- Commits follow [Conventional Commits](https://www.conventionalcommits.org), such as `feat(provider-boat): ...`, one change each. Every commit builds and passes the checks on its own.
- Code, comments, commits and docs are in English.

## Layout

```
crates/
  core/                 workspace-core: logic and ports, no IO, no adapter
    src/vm.rs           VmSpec with its validated name, VmId
    src/port.rs         the Provider, Shell, Profile and Ledger traits
    src/error.rs        Error for the core's failures, Cause for an adapter's
    src/lifecycle.rs    onboard(), boot(), close() and remove()
  cli/                  the workspace binary
    src/main.rs         the commands
    src/config.rs       the config file, and every provider's options
    src/adapters.rs     the provider and profile registries
    src/ledger.rs       FileLedger, one record file per VM
    src/xdg.rs          the XDG base directories
  provider-boat/        workspace-provider-boat: Boat sandboxes
    src/lib.rs          the Provider implementation
    src/options.rs      Boat's options and their defaults
    src/onboard.rs      sign-in and machine type checks
    src/environment.rs  the Boat environment that sandboxes start from
    src/sandbox.rs      creating a sandbox, reading and awaiting its state
    src/shell.rs        Shell over boat ssh
    src/cli.rs          one call to the boat CLI
    src/process.rs      time limits and output limits on boat processes
    src/error.rs        BoatError
```

## Adding a provider or a profile

1. Create `crates/provider-<name>` or `crates/profile-<name>`. It depends on `workspace-core` and implements `Provider` or `Profile`.
2. Add the crate to `crates/cli/Cargo.toml`.
3. Add one line to `PROVIDERS` or `PROFILES` in `crates/cli/src/adapters.rs`.
4. If the provider takes options, give it an `Options` struct that derives `clap::Args` and serde `Deserialize`, then add it as a field of `Providers` in `crates/cli/src/config.rs`. That one field gives it its flags and its `[providers.<name>]` table. Boat's `options.rs` is a model.

The core does not change.
