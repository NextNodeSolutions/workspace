# Boat

With `--provider boat`, each VM is a [Boat](https://boat.dev) sandbox. The adapter drives the `boat` CLI and reads everything it prints. Nothing from Boat reaches your terminal or the core, except through the adapter's own errors.

## Before you start

1. Install the Boat CLI and put it on your `PATH`.
2. Sign in with `boat login`. `boat status` shows whether you are signed in.
3. Run `workspace onboard --provider boat` once. Run it again after you change an option.

## Options

| Flag | Config key | Default | What it sets |
| --- | --- | --- | --- |
| `--boat-type` | `type` | `small` | The machine size: `small` (2 vCPU, 4 GB, half rate), `default` (4 vCPU, 8 GB), `large` (8 vCPU, 16 GB, twice the rate) or `xlarge` (16 vCPU, 32 GB, $100/month plan or higher). |
| `--boat-ttl` | `ttl` | `8h` | How long after boot Boat stops the sandbox, even mid-work. A positive whole number followed by `d`, `h`, `m` or `s`, such as `90m`. |
| `--boat-environment` | `environment` | `workspace` | The Boat environment the sandboxes start from. ASCII letters, digits, dots, dashes and underscores. |

Config keys go under `[providers.boat]`. See [Configuration](../configuration.md).

The TTL is a hard limit. Boat's timer counts time since boot, and cannot tell whether an agent is still working. Stopping a sandbox soon after its agent goes idle is a job for the profile that runs the agent.

## What a sandbox gets from your account

A Boat environment decides what a sandbox receives from your account. Boat's default environment, `base`, can pass your GitHub access, agent logins and secrets. `workspace` keeps its own environment instead, which passes one thing only:

| Toggle | Set to | Why |
| --- | --- | --- |
| Master switch (`safe-for-third-parties`) | off | When on, it withholds everything, the boat key included. |
| GitHub access | off | |
| Secrets (variables and files) | off | |
| Agent logins (Claude, Codex) | off | |
| Boat key | on | The key is scoped to its own sandbox, so the sandbox can stop itself but cannot reach any other. |

`onboard` creates the environment and keeps it in that shape. `boot` refuses to start from it otherwise.

```mermaid
stateDiagram-v2
    state "Missing" as Missing
    state "Everything injected" as Full
    state "Set up, boot allowed" as Ready
    state "Changed by hand" as Changed

    [*] --> Missing
    Missing --> Full: onboard runs boat env new
    Full --> Ready: onboard runs boat env set
    Ready --> Ready: onboard again changes nothing
    Ready --> Changed: someone flips a toggle
    Changed --> Ready: onboard runs boat env set
    Ready --> Missing: boat env rm
```

- `boot` works only in the Set up state. Anywhere else it fails before it creates anything, and says what differs.
- Each `boat env set` creates a new version of the environment. New sandboxes start from the latest version. Running ones keep theirs.
- `onboard` never touches the account's default environment, since your other sandboxes start from it.

## Sandbox states

Boat has more states than the adapter needs, so the adapter groups them:

| Adapter state | Boat's names | Meaning |
| --- | --- | --- |
| Starting | `init`, `provisioning`, `provisioned` (the CLI shows `starting`), `cloning` | Boat is setting the machine up. |
| Up | `ready`, `idle`, `running` | It accepts ssh. |
| Stopping | `stopping`, `archiving` | Boat is snapshotting the disk before it stops the machine. |
| Stopped | `stopped`, `archived` | Nothing runs, and the disk is kept. |
| Gone | answers `not_found` | Deleted, or never existed |
| Other | `error`, `cancelled`, any other | Something went wrong on Boat's side. |

```mermaid
stateDiagram-v2
    [*] --> Starting: boat new
    Starting --> Up: ready
    Starting --> Other: Boat fails
    Up --> Stopping: close, or the TTL runs out
    Stopping --> Stopped: disk snapshotted
    Stopped --> Starting: boat resume
    Up --> Gone: remove
    Stopped --> Gone: remove
    Gone --> [*]
    note right of Stopped
        Boat keeps the disk for 14 days,
        then erases it for good.
    end note
```

A stopped sandbox costs nothing. Within 14 days, `boat resume <id>` brings it back. After that, Boat erases the disk and the sandbox can no longer resume.

## What each operation does

### `onboard`

```mermaid
sequenceDiagram
    autonumber
    participant A as Boat adapter
    participant B as boat CLI
    A->>B: boat status
    Note over A,B: stops unless the CLI is signed in
    A->>B: boat limits
    Note over A,B: stops unless the account can start the machine type
    A->>B: boat env list
    opt the environment is missing
        A->>B: boat env new workspace
        A->>B: boat env list
    end
    Note over A,B: stops if it is the default environment
    opt a toggle differs
        A->>B: boat env set workspace, every toggle
    end
    A->>B: boat env list
    Note over A,B: stops unless the sandbox would get only its boat key
```

It then prints the machine type, the TTL, and whether it created, reset or kept the environment:

```console
$ workspace onboard --provider boat
Boat is ready: `workspace boot` starts small sandboxes, which Boat stops 8h after boot.
They start from the Boat environment `workspace` (created now), which gives them their own boat key and nothing else from the account.
```

### `boot`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: boot
    A->>B: boat env list
    Note over A,B: stops unless the environment is set up
    A->>B: boat new --type small --ttl 28800 --environment workspace
    B-->>A: first line, with the sandbox id
    A-->>Core: sandbox id
    Note over A,B: boat new runs on until the sandbox is ready
    Core->>Core: record the id
    Core->>A: shell
    loop every 2 s, up to 5 min, while Starting
        A->>B: boat info ID
    end
    A-->>Core: a shell, once the sandbox is Up
```

The adapter returns the id from the first line `boat new` prints, about a second before the sandbox is ready. That keeps the window without a record as short as possible. `shell` then waits through the starting states, and fails at once on a stopped, deleted or failed sandbox.

### `Shell::run`, used by profiles

```mermaid
sequenceDiagram
    autonumber
    participant P as Profile
    participant A as Boat adapter
    participant B as boat ssh
    participant S as Sandbox
    P->>A: run(script, stdin)
    A->>B: boat ssh ID -- bash -c SCRIPT
    A-)B: stdin, written from its own thread
    B->>S: runs the script
    S-->>B: stdout, stderr, exit status
    B-->>A: stdout and stderr, up to 64 MiB each
    alt the script exits 0
        A-->>P: stdout
    else the script fails
        A-->>P: error with boat's reason, then the script's stderr
    end
```

A script has no time limit, since provisioning can take long. It fails once stdout or stderr passes 64 MiB.

### `close`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: close(ID)
    A->>B: boat stop ID
    alt Boat gave no answer
        A-->>Core: error, at once
    else Boat accepted, or refused because it is already stopped or gone
        loop every 2 s, up to 15 min, while Stopping
            A->>B: boat info ID
        end
        A-->>Core: done once Stopped or Gone, else an error
    end
```

### `remove`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: remove(ID)
    A->>B: boat delete ID --yes
    alt deleted, or not_found
        A-->>Core: done
    else any other failure
        A-->>Core: error, so the record stays
    end
```

## One call to the boat CLI

Every call adds `--json --no-update`, and one module owns every `boat` process:

```mermaid
stateDiagram-v2
    state "Running" as Running
    state "Killed, stalled" as Stalled
    state "Exited" as Exited
    state "Too much output" as TooMuch
    state "Success, JSON read" as Success
    state "Refused, with a code" as Refused
    state "Failed, with a message" as Failed

    [*] --> Running: spawn
    Running --> Exited: the process ends
    Running --> Stalled: its time limit passes
    Exited --> TooMuch: a stream passed 64 MiB
    Exited --> Success: exit 0
    Exited --> Refused: the last line is an error with a code
    Exited --> Failed: any other failure
```

- **Refused** means the Boat API said no. Its code drives decisions: `not_found` means the sandbox is gone.
- **Failed** carries the message from boat's last JSON line, or its stderr when there is none.
- **Stalled**: after a kill, the threads that read the pipes are left behind, so a leftover child process that holds a pipe open cannot delay the error.
- **Too much output**: past the limit, the rest of a stream is read and dropped, so the command never blocks on a full pipe.

## Limits

| What | Limit |
| --- | --- |
| A call that only talks to the Boat API (`info`, `stop`, `delete`, `status`, `limits`, `env`) | Killed after 60 s |
| `boat new` | Killed if it prints no id within 60 s. The error tells you to check `boat list`. |
| Waiting for a sandbox to start | 5 min |
| Waiting for a sandbox to stop | 15 min, to leave time for the disk snapshot |
| Polling while waiting | Every 2 s |
| A script run over ssh | No time limit, 64 MiB per output stream |
| A stopped sandbox's disk | Kept 14 days |

## Design choices

- **ssh, not exec.** `boat exec` takes no stdin, caps a command at 600 s, and Boat keeps the command and output of every exec for 90 days. Secrets would end up stored there. `boat ssh` forwards stdin and has none of these limits.
- **`small` by default**, per the operator's sizing rule. A larger type is a choice you make in the config or with a flag.
- **The VM name lives only in the record.** Boat names sandboxes itself.

## What the adapter relies on

Observed with Boat CLI 1.0.39. If Boat changes one of these, the adapter needs a change too.

- `boat new` prints `{"event":"created","id":...}` first, and `ready` about a second later.
- A failing command ends its stdout with `{"error":...,"event":"error"}`. That line has a `code` only when the Boat API refused.
- `boat ssh` forwards stdin and the remote stderr, and quotes each argument for the remote shell. It exits 1 whatever the script's exit code, with the reason in that last line.
- `boat stop` on a stopping sandbox returns the same operation. On a stopped one it fails with `stop_failed`, and on a deleted one with `not_found`.
- `boat delete` works on a running or a stopped sandbox. `boat info` answers `not_found` right after.
- `boat env list` gives each environment's toggles, its `effective` values and `isDefault`. `boat env new` starts with everything injected. `boat env set` takes `--safe-for-third-parties`, `--github`, `--secrets`, `--agents-credentials` and `--boat-credentials`.
- A sandbox's boat key is scoped to that sandbox only.
