# workspace

`workspace` boots a VM on a provider, records it, and provisions it with profiles. Later it closes the VM, which stops it, or removes it for good. The core knows no provider and no profile. Both are adapters that the command line picks by name.

One provider exists, Boat. No profile exists yet, so `boot` brings up a bare VM.

## Build

```sh
cargo build --release
ln -s "$PWD/target/release/workspace" "$HOME/.local/bin/workspace"
```

## Commands

| Command | What it does |
| --- | --- |
| `workspace boot <name> --provider <provider> [--profile <profile>]...` | Boots a VM, records it, waits until it answers, applies each profile in order, then prints the VM id. |
| `workspace close <id>` | Stops a VM that `boot` recorded, on the provider named in its record. The VM still exists and its record stays. |
| `workspace remove <id>` | Deletes a recorded VM and its data for good, then deletes the record. |

`<name>` must be a DNS label of 1 to 63 lowercase letters, digits or hyphens, with no hyphen at either end.

The profile registry is still empty:

```console
$ workspace boot dev --provider boat --profile pi
error: unknown profile `pi`; available: none
```

## Providers

### Boat

`--provider boat` needs the Boat CLI on PATH, signed in. `boat status` tells.

- `boot` creates a `small` sandbox with an 8 hour TTL. When the TTL runs out, Boat stops the sandbox even mid-work, because its timer cannot tell when an agent goes idle.
- `shell` runs scripts over `boat ssh`, stdin piped through. `boat exec` takes no stdin, and Boat keeps the command and output of every exec for 90 days, so the adapter does not use it. A script has no time limit, but it fails once its stdout or stderr passes 64 MiB.
- `close` runs `boat stop` and waits until Boat has snapshotted the disk and stopped the sandbox. Boat keeps the disk of a stopped sandbox for 14 days, for free, and `boat resume <id>` brings it back meanwhile. On day 14 Boat erases the disk, and the sandbox can no longer resume.
- `remove` runs `boat delete`, which erases the sandbox and all of its data.

Every other `boat` call is killed after 60 s without an answer.

## Records

`boot` writes one file per VM in `$XDG_DATA_HOME/workspace/vms/`, or in `~/.local/share/workspace/vms/` when `XDG_DATA_HOME` is unset. The file is named after the VM id and holds the provider and the name:

```
provider=boat
name=dev
```

`boot` writes the record as soon as the provider returns the id, before it waits on the VM. If you interrupt a boot, the record is already on disk. List the directory to find the VM, then run `workspace close <id>` or `workspace remove <id>`.

## Layout

```
crates/
  core/               workspace-core: domain and ports, no IO, no adapter dependency
    src/vm.rs         VmSpec with its validated name, VmId
    src/port.rs       the Provider, Shell, Profile and Ledger traits
    src/error.rs      Error for the core's failures, Cause for an adapter's
    src/lifecycle.rs  boot(), close() and remove()
  cli/                the workspace binary: picks adapters by name and calls the core
    src/main.rs       boot, close and remove commands
    src/adapters.rs   provider and profile registries: boat, no profile yet
    src/ledger.rs     FileLedger, the record files described above
  provider-boat/      workspace-provider-boat: Boat sandboxes through the boat CLI
```

This is a ports and adapters layout, the Rust version of hexagonal architecture. The core's `Cargo.toml` lists no adapter, so the compiler rejects any import of one.

## Ports

| Trait | Implemented by | Contract |
| --- | --- | --- |
| `Provider` | Boat, AWS, Cloudflare, a local hypervisor | `boot` returns the id as soon as the provider assigns one. `shell` waits until the VM answers. `close` stops the VM and succeeds when it is already stopped or gone. `remove` deletes it for good and succeeds when it is already gone. |
| `Shell` | the provider's transport, ssh or an API | `run(script, stdin)` returns stdout and fails on a non-zero exit. Secrets and files travel on stdin, never in the script. |
| `Profile` | Pi with its skills, a cron setup | `provision(&dyn Shell)` must be safe to run twice. A profile sees only the shell, so it works the same on every provider. |
| `Ledger` | `FileLedger` in the CLI | `record` runs before anything waits on the VM. `forget` runs once the VM is removed. |

Adapters report failures as a `Cause`, a boxed error that the core attaches to its own `Error` without reading it. No provider type reaches the core.

## Adding an adapter

1. Create `crates/provider-<name>` or `crates/profile-<name>`, depending on `workspace-core`, and implement the trait.
2. Add the crate to `crates/cli/Cargo.toml`.
3. Add one line to `PROVIDERS` or `PROFILES` in `crates/cli/src/adapters.rs`.

The core does not change.

## Guarantees

- `boot` writes a VM's record as soon as its provider returns the id. One window remains, inside `Provider::boot`, between the provider allocating the VM and returning its id. Adapters must return as early as they can.
- A failure after boot leaves the VM running, and the error names it. That covers recording, reaching the VM and provisioning. Nothing closes a VM automatically.
- If recording itself fails, the error message is the only place the id appears.
- `close` keeps the record, since a stopped VM still exists. `remove` keeps it when the provider fails, so you can run it again.

## Development

```sh
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace turns on `clippy::pedantic` and forbids `unsafe`. `clippy.toml` caps functions at 50 lines and cognitive complexity at 15. There are no tests yet.
