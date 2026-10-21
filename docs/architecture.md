# Architecture

`workspace` follows ports and adapters. A small core holds the logic and knows no provider or profile. Adapters plug into it through traits, called ports, and the binary wires them together.

## The pieces

```mermaid
flowchart LR
    user([You]) --> cli
    subgraph binary [the workspace binary]
        cli["cli<br/>commands, config, records"]
        core["core<br/>logic and ports"]
        adapters["adapters<br/>one crate per provider or profile"]
    end
    cli -- calls --> core
    cli -- builds by name --> adapters
    adapters -. implement the ports of .-> core
    adapters -- talk to --> outside[[the provider]]
    cli -- reads and writes --> files[(config file<br/>and records)]
```

- **The core** does no IO and depends on no adapter, so the compiler rejects any import of one.
- **The cli** is the only part that knows every adapter. It builds them by the names given on the command line.
- **An adapter** depends on the core only. Adding one never changes the core.

## Ports

| Port | Contract |
| --- | --- |
| `Provider` | `setup` prepares the account and is safe to repeat. `create` returns the VM id as soon as the provider assigns one. `shell` waits until the VM answers. `stop` stops the VM, and succeeds when it is already stopped or gone. `remove` deletes it, and succeeds when it is already gone. |
| `Shell` | `run(script, stdin)` runs a script on the VM and returns its stdout. It fails on a non-zero exit. Secrets and files travel on stdin, never in the script. |
| `Profile` | `provision(shell)` sets up the VM. It must be safe to run twice. It sees only the shell, so it works the same on every provider. |
| `Ledger` | `record` runs before anything waits on the VM. `forget` runs once the VM is removed. The cli implements it with one file per VM. |

An adapter reports a failure as an opaque cause. The core wraps it in its own error without reading it, so no provider type leaks into the core. The binary prints the error, then one line per underlying cause.

## Commands step by step

### `setup`

```mermaid
sequenceDiagram
    autonumber
    actor You
    participant CLI as cli
    participant Core as core
    participant P as Provider
    You->>CLI: workspace setup
    CLI->>CLI: load the config file, then put the flags over it
    CLI->>CLI: pick the provider, then build it with those options
    CLI->>Core: setup
    Core->>P: setup()
    P-->>Core: summary
    Core-->>CLI: summary
    CLI->>CLI: write the provider into the config file
    CLI-->>You: prints the summary
```

The provider becomes the default only once its setup succeeds. A provider that is not ready never becomes the default.

### `start`

```mermaid
sequenceDiagram
    autonumber
    actor You
    participant CLI as cli
    participant Core as core
    participant P as Provider
    participant L as Ledger
    participant Pr as Profiles
    participant S as Shell
    You->>CLI: workspace start NAME
    CLI->>CLI: check the name, build the provider and the profiles
    CLI->>Core: start
    Core->>P: create(spec)
    P-->>Core: VM id, before the VM is ready
    Core->>L: record(id, spec)
    Core->>P: shell(id)
    Note over P: waits until the VM answers
    P-->>Core: a shell on the VM
    loop each profile, in order
        Core->>Pr: provision(shell)
        Pr->>S: run(script, stdin)
    end
    Core-->>CLI: VM id
    CLI-->>You: prints the VM id
```

The record is written right after the provider returns the id, before anything waits. If you interrupt a start, the VM is already recorded.

### `stop` and `remove`

```mermaid
sequenceDiagram
    autonumber
    actor You
    participant CLI as cli
    participant L as Ledger
    participant Core as core
    participant P as Provider
    You->>CLI: workspace stop VM, or workspace remove VM
    CLI->>L: find the record whose id or else name is VM
    L-->>CLI: the VM id and its provider
    CLI->>CLI: build that provider
    alt stop
        CLI->>Core: stop
        Core->>P: stop(ID)
        Note over L: the record stays
    else remove
        CLI->>Core: remove
        Core->>P: remove(ID)
        Core->>L: forget(ID), only once the VM is deleted
    end
```

## How a start can end

Each step of a start can fail. What is left depends on how far it got.

```mermaid
stateDiagram-v2
    state "Starting the VM" as Allocating
    state "Recording it" as Recording
    state "Waiting until it answers" as Waiting
    state "Applying profiles" as Provisioning
    state "Ready, id printed" as Ready
    state "Failed, nothing to clean up" as NoVm
    state "Failed, VM runs without a record" as Unrecorded
    state "Failed, VM runs and is recorded" as Recorded

    [*] --> Allocating
    Allocating --> Recording: id returned
    Allocating --> NoVm: provider fails
    Recording --> Waiting: record written
    Recording --> Unrecorded: record fails
    Waiting --> Provisioning: VM answers
    Waiting --> Recorded: VM never answers
    Provisioning --> Ready: every profile succeeds
    Provisioning --> Recorded: a profile fails
    Ready --> [*]
```

Nothing is stopped for you. A failed VM keeps running and the error names it, so you decide whether to fix it, stop it or remove it. When the record itself fails, the error message is the only place the id appears.

## What a failure leaves behind

| Failure | What is left |
| --- | --- |
| The VM name is invalid | Nothing |
| The VM name is already recorded | Nothing |
| The provider cannot start the VM | Nothing |
| The record cannot be written | A running VM with no record. The error holds its id. |
| The VM never answers | A running, recorded VM |
| A profile fails | A running, recorded VM, as the failed step left it |
| Setup stops | The account as far as setup got, and the config file as it was. Run it again. |
| The stop does not finish | The VM may still run. The record stays. |
| The delete fails | The VM may still exist. The record stays, so `remove` can run again. |
| The VM is deleted but its record is not | No VM. Run `remove` again to delete the record. |

## Records

`start` writes one file per VM, named after the VM id, in `$XDG_DATA_HOME/workspace/vms/`, or in `~/.local/share/workspace/vms/` when `XDG_DATA_HOME` is unset. The file names the provider and the VM name:

```
provider=boat
name=dev
```

The file is written under a temporary name, then renamed, so it is never half written. After an interrupted start, `workspace list` shows the VM.

Since the record holds the name, `stop` and `remove` take the name in place of the id. A name means one VM: `start` refuses a name that is already recorded, before it creates anything. If older records share a name, a command given that name fails and lists their ids, so you pick one.

```mermaid
stateDiagram-v2
    direction LR
    state "No record" as None
    state "Record on disk" as Present
    [*] --> None
    None --> Present: start gets an id
    Present --> Present: stop, or a failed remove
    Present --> None: remove succeeds
```

## Guarantees

- The record is written as soon as the provider returns the id. The only gap is inside `Provider::create`, between the provider starting the VM and returning its id, so adapters return the id as early as they can.
- A failure after the id never stops or deletes the VM.
- `stop` keeps the record, since a stopped VM still exists.
- `remove` deletes the record only after the provider confirms the VM is gone.
- Stopping a stopped or deleted VM succeeds, and so does removing a deleted one, so every command can be repeated.
