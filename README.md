# workspace

`workspace` boots VMs on a cloud provider, sets them up with profiles, then stops or deletes them. It records every VM it boots on your disk, so a VM that runs and costs money cannot be lost.

## Quick start

```sh
cargo build --release
ln -s "$PWD/target/release/workspace" "$HOME/.local/bin/workspace"

workspace onboard --provider boat     # once, to prepare the provider account
workspace boot dev --provider boat    # prints the VM id
workspace close <id>                  # stops the VM and keeps its disk
workspace remove <id>                 # deletes the VM and its data for good
```

`workspace --help` lists every command and option.

## How it works

```mermaid
stateDiagram-v2
    state "Account not prepared" as Unprepared
    state "Account prepared" as Prepared
    state Prepared {
        state "No VM" as NoVm
        [*] --> NoVm
        NoVm --> Running: workspace boot
        Running --> Stopped: workspace close
        Stopped --> Running: resumed on the provider
        Running --> NoVm: workspace remove
        Stopped --> NoVm: workspace remove
    }
    [*] --> Unprepared
    Unprepared --> Prepared: workspace onboard
```

1. `onboard` prepares a provider account once, for example by creating what new VMs start from. Running it again is safe.
2. `boot` starts a VM, records it on your disk, waits until it answers, applies each profile in order, then prints the VM id.
3. `close` stops the VM. Its disk and its record stay, so the VM can resume.
4. `remove` deletes the VM and its data, then its record.

A provider is where VMs run, such as [Boat](docs/providers/boat.md). A profile is what a VM is for, such as an agent and its tools. Both are plugins that the core knows nothing about, as [Architecture](docs/architecture.md) explains. Their options come from flags or a config file, as [Configuration](docs/configuration.md) explains. To change the code, read [Development](docs/development.md).
