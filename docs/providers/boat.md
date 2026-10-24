# Boat

On the `boat` provider, each VM is a [Boat](https://boat.dev) sandbox. The adapter drives the `boat` CLI, reads everything it prints, and turns every failure into its own error. Boat's output never reaches your terminal or the core, but for the shell that `start` opens for you.

## Before you start

1. Install the Boat CLI, put it on your `PATH`, and sign in with `boat login`.
2. Run `workspace setup --provider boat`. It prepares your account and makes Boat the default provider. Run it again whenever you change an option.

## Options

| Flag | Config key | What it sets |
| --- | --- | --- |
| `--boat-type` | `type` | The machine size. It defaults to the smallest. A larger one is a choice you make here. |
| `--boat-ttl` | `ttl` | How long after it starts Boat stops the sandbox, such as `90m` or `8h`. The timer starts again on each resume. |
| `--boat-environment` | `environment` | The Boat environment that sandboxes start from |
| `--boat-from` | `from` | The saved snapshot that new sandboxes start from, as `workspace snapshot` saves it. Without it, they start from Boat's base image. |

`workspace start --help` shows the accepted values and the defaults. `setup` checks that your Boat plan allows the machine type. The machine type is chosen when `start` creates a sandbox. The TTL and the environment also apply each time `start` resumes one.

A disposable VM is created with `--no-snapshots`: Boat keeps no disk for it, so a stop erases it and nothing is left to pay for or to clean up. Boat charges per second while a sandbox runs; create, resume, fork and stop cost nothing, and a stopped kept sandbox costs nothing either. Saved snapshots are included in the plan, ten of them on the $20 plan, and each one beyond that costs $1.70 a month, so `workspace snapshot` reuses one name rather than inventing new ones.

The TTL is a ceiling, not an idle timer. Boat stops the sandbox when it runs out, even mid-work, because its timer cannot tell whether an agent is still busy. Stopping a sandbox once its agent goes idle is the job of the profile that runs the agent.

## What a sandbox gets from your account

A Boat environment decides what a sandbox receives from your account. Boat's default environment can pass your GitHub access, agent logins and secrets. `workspace` uses an environment of its own, so that a sandbox gets one thing only:

| What Boat can pass | In workspace's environment |
| --- | --- |
| GitHub access | off |
| Secrets, as variables and files | off |
| Agent logins | off |
| The sandbox's own boat key | on |

The boat key reaches only its own sandbox, so the sandbox can stop itself and nothing else. Boat also has a master switch that withholds everything. It stays off, since it would withhold that key too.

```mermaid
stateDiagram-v2
    state "Missing" as Missing
    state "Everything injected" as Full
    state "Set up, start allowed" as Ready
    state "Changed by hand" as Changed
    state "Deleted, name still taken" as Deleted

    [*] --> Missing
    Missing --> Full: setup runs boat env new
    Full --> Ready: setup runs boat env set
    Ready --> Ready: setup again changes nothing
    Ready --> Changed: someone flips a toggle
    Changed --> Ready: setup runs boat env set
    Ready --> Deleted: boat env rm
    Deleted --> [*]: setup needs another name
```

- `start` creates a sandbox only when the environment is set up. In any other state it fails before it creates anything, and says what differs.
- Each change creates a new version of the environment. New sandboxes start from the latest one, and running ones keep theirs.
- `setup` never touches the account's default environment, since your other sandboxes start from it.
- Do not delete the environment with `boat env rm`. Boat only marks it as deleted and keeps its name taken for good, so `setup` can no longer create an environment by that name, and you have to name another one with `--boat-environment` or `environment`.

## Sandbox states

Boat names more states than the adapter needs, so the adapter folds them into these:

```mermaid
stateDiagram-v2
    state "Error or unknown" as Other
    [*] --> Starting: boat new
    Starting --> Up: ready
    Starting --> Other: Boat fails
    Up --> Stopping: stop, or the TTL runs out
    Stopping --> Stopped: disk snapshotted
    Stopped --> Starting: start, through boat resume
    Up --> Gone: remove
    Stopped --> Gone: remove
    Gone --> [*]
    note right of Stopped
        Boat keeps the disk for a while,
        then erases it for good.
    end note
```

- **Up** means the sandbox accepts ssh.
- **Stopping** means Boat is snapshotting the disk before it stops the machine.
- **Stopped** means nothing runs and the disk is kept, so `workspace start` can bring the sandbox back. Once Boat erases the disk after its retention period, the sandbox can no longer resume.
- **Gone** means Boat answers that the sandbox does not exist.

## What each operation does

### `setup`

```mermaid
sequenceDiagram
    autonumber
    participant A as Boat adapter
    participant B as boat CLI
    A->>B: boat status
    Note over A,B: stops unless the CLI is signed in
    A->>B: boat limits
    Note over A,B: stops unless the plan allows the machine type
    A->>B: boat env list
    opt the environment is missing
        A->>B: boat env new
        A->>B: boat env list
    end
    Note over A,B: stops if it is the default environment
    opt a toggle differs
        A->>B: boat env set, with every toggle
    end
    A->>B: boat env list
    Note over A,B: stops unless the sandbox would get only its boat key
```

It then prints what `start` will use, and whether it created, reset or kept the environment.

### `create`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: create
    A->>B: boat env list
    Note over A,B: stops unless the environment is set up
    A->>B: boat new, with the type, the TTL and the environment
    B-->>A: first line, with the sandbox id
    A-->>Core: sandbox id
    Note over A,B: boat new runs on in the background until the sandbox is ready
    Core->>Core: record the id
    Core->>A: shell
    loop while Starting, within a time limit
        A->>B: boat info
    end
    A-->>Core: a shell, once the sandbox is Up
```

`boat new` prints the sandbox id on its first line, before the sandbox is ready. The adapter returns it at once, which keeps the time without a record short. `shell` then waits through the starting states, and fails at once on a deleted or failed sandbox. It lets a stopped state pass for a short window, since Boat may still report a sandbox it just resumed as stopped, then fails. Boat names sandboxes itself, so the VM name lives only in the record.

### `resume`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: resume(ID)
    loop while Stopping, within a time limit
        A->>B: boat info
    end
    alt Up or Starting
        A-->>Core: done, since it runs
    else Stopped
        A->>B: boat env list
        Note over A,B: stops unless the environment is set up
        A->>B: boat resume --no-wait, with the TTL and the environment
        alt Boat accepted
            A-->>Core: done
        else Boat refused
            A->>B: boat info
            A-->>Core: done if it now runs, else the refusal
        end
    else Gone, or another state
        A-->>Core: error that names the state
    end
    Core->>A: shell
    Note over A,B: waits for Up, as after create, past a stale Stopped
```

Boat resumes only a stopped sandbox, so one that is still stopping is waited for first. `boat resume` would wait until the sandbox is ready, so the adapter passes `--no-wait`: the call returns once Boat accepts, within its time limit, and `shell` waits for the sandbox as after `create`. Boat may still report the sandbox as stopped for a moment after it accepts, and that same wait lets it pass, so a resume reads the state no more than it must.

A resumed sandbox gets what its environment holds at that moment, so the adapter checks the environment first, as before a create. It then pins the sandbox to that environment, so the sandbox gets the version just checked, even if someone upgraded the environment while the sandbox was stopped. The sandbox keeps its machine type, and its TTL starts again from the resume. A gone sandbox cannot come back: `start` then says that `workspace remove` forgets its record.

### `snapshot`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: snapshot(ID, NAME)
    A->>B: boat snapshot ID NAME
    Note over A,B: saves the running or stopped sandbox's disk, replacing a saved snapshot of that name
    A-->>Core: done
```

Saving uploads the disk, which takes seconds to minutes, so the call has a long time limit. `boat new --from NAME` then starts new sandboxes from it, with the environment and `--no-snapshots` as usual. A sandbox created from a snapshot is ready in a few seconds with everything the snapshot holds, profiles' work included.

### `attach`

```mermaid
sequenceDiagram
    autonumber
    actor You
    participant A as Boat adapter
    participant B as boat ssh
    participant S as Sandbox
    A->>B: boat ssh ID, on your terminal
    B->>S: opens a shell
    You->>S: work in the shell
    You->>B: leave the shell
    B-->>A: exits, and the sandbox keeps running
```

`boat ssh` runs without `--json` here, since its output is the session you read. It has no time limit, since the session lasts as long as you want. Its exit status is that of the last command typed in the session, and Boat's own refusals exit the same way. After a non-zero exit, the adapter therefore reads the state back: a sandbox that still runs means the session ended normally, and one that no longer runs means there was no session to give.

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
    B-->>A: stdout and stderr, within a size limit
    alt the script exits 0
        A-->>P: stdout
    else the script fails
        A-->>P: error with boat's reason, then the script's stderr
    end
```

- **Why ssh.** `boat exec` takes no stdin, and Boat stores the command and output of every exec. Secrets could only travel in the command, and would end up stored. `boat ssh` forwards stdin.
- **No time limit.** Provisioning can take long, so a script runs until it ends. Its output has a size limit instead.
- **Telling failures apart.** `boat ssh` exits the same way whether the script failed or Boat refused. Boat's last line tells which, and the adapter reads it.

### `stop`

```mermaid
sequenceDiagram
    autonumber
    participant Core as core
    participant A as Boat adapter
    participant B as boat CLI
    Core->>A: stop(ID)
    A->>B: boat stop
    alt Boat gave no answer
        A-->>Core: error, at once
    else Boat accepted, or refused because it is already stopped or gone
        loop while Stopping, within a time limit
            A->>B: boat info
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
    A->>B: boat delete
    alt deleted, or not found
        A-->>Core: done
    else any other failure
        A-->>Core: error, so the record stays
    end
```

## One call to the boat CLI

Every call skips boat's update check, and every call but the interactive shell asks for JSON output. One module owns every `boat` process, and each call ends in one of these states:

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
    Exited --> TooMuch: a stream passed the size limit
    Exited --> Success: exit 0
    Exited --> Refused: the last line is an error with a code
    Exited --> Failed: any other failure
```

- **Time limit.** A call that only talks to the Boat API gets one, since it should answer within seconds. `boat ssh` gets none.
- **Refused** means the Boat API said no, and its code drives decisions. For example, a not-found code means the sandbox is gone, which `stop` and `remove` treat as success.
- **Failed** carries the message from boat's last line, or its stderr when there is no such line.
- **Stalled.** After a kill, the threads reading the pipes are left behind, so a leftover child process that holds a pipe open cannot delay the error. Boat may still carry out a request it never answered, and the error says so.
- **Too much output.** Past the limit, the rest of a stream is read and dropped, so the command never blocks on a full pipe.
