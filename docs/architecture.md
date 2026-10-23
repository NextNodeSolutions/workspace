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
| `Provider` | `setup` prepares the account and is safe to repeat. `create` returns the VM id as soon as the provider assigns one. `resume` makes a stopped VM run again, and succeeds when it already runs. `shell` waits until the VM answers. `attach` hands your terminal to a shell on the VM until you leave it. `stop` stops the VM, and succeeds when it is already stopped or gone. `remove` deletes it, and succeeds when it is already gone. |
| `Shell` | `run(script, stdin)` runs a script on the VM and returns its stdout. It fails on a non-zero exit. Secrets and files travel on stdin, never in the script. |
| `Profile` | `provision(shell)` sets up the VM. It must be safe to run twice. It sees only the shell, so it works the same on every provider. It may read this machine, such as the config of the agent it installs, and sends what it reads on stdin. |
| `Ledger` | `record` runs before anything waits on the VM. `forget` runs once the VM is removed. The cli implements it with one file per VM. |

A `Provider` reports a failure as an opaque cause, with a reason the core can act on: the account needs setup again, the provider no longer has the VM, or anything else. The core never reads the cause. It picks its error from the step and the reason, so no provider type leaks into the core, and the cli can say which command to run next. A VM the provider no longer has counts as stopped and as removed, so `stop` and `remove` can be repeated whatever the provider answers. The binary prints the error, then one line per underlying cause.

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
    CLI->>CLI: lock NAME, waiting while another start holds it
    CLI->>L: find the record named NAME
    CLI->>CLI: build its provider, or else the default one, and the profiles with their options
    CLI->>Core: start
    alt NAME has no record
        Core->>P: create(spec)
        P-->>Core: VM id, before the VM is ready
        Core->>L: record(id, spec)
    else NAME is recorded
        Core->>P: resume(id)
        Note over P: succeeds at once when the VM runs
    end
    Core->>P: shell(id)
    Note over P: waits until the VM answers
    P-->>Core: a shell on the VM
    loop each profile, in order
        Core->>Pr: provision(shell)
        Pr->>S: run(script, stdin)
    end
    Core-->>CLI: VM id
    CLI->>CLI: unlock NAME
    alt with -d
        CLI-->>You: prints the VM id
    else without -d
        CLI->>Core: attach(id)
        Core->>P: attach(id)
        P-->>You: a shell on the VM, until you leave it
        CLI-->>You: says that the VM still runs
    end
```

The record is written right after the provider returns the id, before anything waits. If you interrupt a start, the VM is already recorded.

A recorded name never gets a second VM: `start` resumes the one it has, on the provider that created it. Starting a running VM therefore changes nothing. Two starts of one name at the same moment do not create two VMs either: the second waits until the first has recorded its VM and applied its profiles, then resumes that VM. The lock lasts only as long as the process that holds it, so an interrupted start never blocks the next one. Profiles apply each time you name them, since they are safe to repeat. Without `--profile`, nothing is applied again, and a resumed VM keeps on its disk what earlier profiles installed. Provider options apply when `start` creates a VM, and each provider's page says which ones also apply when it resumes one. `--provider` cannot move a recorded VM: naming another provider than its record's is an error.

Without `-d`, `start` ends by handing your terminal to a shell on the VM. Leaving the shell leaves the VM running, since an agent may still work on it, and `start` reminds you that it does. Without a terminal, such as in a script, `start` fails before it creates anything unless `-d` asks for the id instead.

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
    state "Creating the VM" as Allocating
    state "Resuming the recorded VM" as Resuming
    state "Recording it" as Recording
    state "Waiting until it answers" as Waiting
    state "Applying profiles" as Provisioning
    state "Ready, id printed" as Ready
    state "Failed, nothing to clean up" as NoVm
    state "Failed, VM runs without a record" as Unrecorded
    state "Failed, VM runs and is recorded" as Recorded
    state "Failed, VM and record as they were" as Kept

    [*] --> Allocating: the name has no record
    [*] --> Resuming: the name is recorded
    Resuming --> Waiting: the provider resumes it
    Resuming --> Kept: the provider cannot
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
| The provider needs setup first | Nothing new. Run `setup`, then start again. |
| The provider no longer has the VM | Its record, which `remove` forgets. |
| The recorded VM cannot resume | The VM as it was, and its record. If the provider no longer has it, `remove` forgets the record. |
| The provider cannot start the VM | Nothing |
| The record cannot be written | A running VM with no record. The error holds its id. |
| The VM never answers | A recorded VM, which may still run |
| The shell cannot open | The VM as it was, and its record. `start` opens the shell again, resuming the VM first if it stopped. |
| A profile fails | A running, recorded VM, as the failed step left it |
| Setup stops | The account as far as setup got, and the config file as it was. Run it again. |
| The stop does not finish | The VM may still run. The record stays. |
| The delete fails | The VM may still exist. The record stays, so `remove` can run again. |
| The VM is deleted but its record is not | No VM. Run `remove` again to delete the record. |

## Records

`start` writes one file per VM, named after the VM id, under the XDG data directory. The file names the provider and the VM name:

```
provider=boat
name=dev
```

The file is written under a temporary name, then renamed, so it is never half written. After an interrupted start, `workspace list` shows the VM.

A file that cannot be read, say after a hand edit, never hides the others. `list` prints every VM it can read, then names each file it cannot and fails, so a script notices. Until that file is fixed or deleted, a name that no readable record holds cannot be looked up, since the unreadable file might hold it, and `start` would create a second VM. A name that a readable record holds works as usual.

Since the record holds the name, `stop` and `remove` take the name in place of the id. A name means one VM: when a name is recorded, `start` resumes that VM rather than create another. If older records share a name, a command given that name fails and lists their ids, so you pick one.

```mermaid
stateDiagram-v2
    direction LR
    state "No record" as None
    state "Record on disk" as Present
    [*] --> None
    None --> Present: start gets an id
    Present --> Present: start, stop, or a failed remove
    Present --> None: remove succeeds
```

## Guarantees

- The record is written as soon as the provider returns the id. The only gap is inside `Provider::create`, between the provider starting the VM and returning its id, so adapters return the id as early as they can.
- A failure after the id never stops or deletes the VM.
- `stop` keeps the record, since a stopped VM still exists, and `start` resumes it rather than create another.
- `remove` deletes the record only after the provider confirms the VM is gone.
- Stopping a stopped or deleted VM succeeds, and so does removing a deleted one, so every command can be repeated.
