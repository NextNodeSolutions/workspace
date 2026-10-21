# Boat adapter rules

This crate runs VMs as Boat sandboxes, through the `boat` CLI, which owns sign-in. `docs/providers/boat.md` shows how each operation works. These rules keep the adapter safe and correct when Boat changes.

## Talking to boat

- **One way in.** Every `boat` process starts in the module that runs boat, which asks for JSON output and skips the update check. Never spawn `boat` anywhere else.
- **Bound everything.** A call that only talks to the Boat API has a time limit. Captured output has a size limit, and the rest is drained, so `boat` never blocks on a full pipe. After a kill, the pipe readers are left behind instead of joined, since a leftover child process may hold a pipe open. A script over ssh has no time limit, on purpose.
- **Read only what you use.** Boat's JSON is read into structs that hold only the fields the adapter uses, and every other field is ignored. Boat can then add fields freely. Never deny unknown fields on Boat's output, unlike the config file.
- **Branch on codes and state, never on wording.** A refusal from the Boat API carries a code, and the adapter decides on that code: not found means gone, for example. When Boat gives no code, read the state back. That is why the adapter checks whether an environment exists by listing environments, not by parsing an error message.
- **Fail fast on silence.** A call that got no answer at all, as opposed to a refusal, fails at once. Do not chain more calls to a CLI that just stalled.
- **Keep Boat inside.** Nothing `boat` prints reaches the terminal. Every failure becomes this crate's error, whose message names the `boat` command and says what to do next. Only the provider and its options are public.

## Sandbox lifecycle

- **Return the id first.** `create` returns the id from the first line of `boat new` that carries one, and lets `boat new` finish in the background. Never wait for readiness inside `create`: nothing records the sandbox until the id comes back.
- **Fold states, never panic on them.** Boat's state names map onto the adapter's few states. A name the adapter does not know falls into the error-or-unknown state, and operations treat that state as a failure.
- **Stop and delete can be repeated.** Boat refuses to stop a sandbox that is already stopped or gone, so `stop` reads the state after `boat stop`, and that refusal counts as success. `remove` treats not found as success.
- **Scripts go over ssh, as one argument.** `boat exec` takes no stdin, and Boat stores the command and output of every exec, so secrets could end up stored there. `boat ssh` quotes each argument for the remote shell, so the script travels as a single argument to bash. `boat ssh` exits the same way for a failed script and for a refused request, and the code on its last line tells them apart.

## Account and environment

- **Least privilege.** Sandboxes start from workspace's own environment, which passes only the sandbox's own boat key. A sandbox needs that key to stop itself, and the key reaches nothing else. Do not turn on GitHub access, secrets or agent logins there. A profile that needs a credential passes it on stdin.
- **Check before creating.** `create` reads the environment's effective toggles, and refuses to create a sandbox unless they match exactly. A hand edit to the environment therefore cannot give sandboxes more than intended.
- **Setup owns only its environment.** It creates the environment when it is missing, and sets the toggles only when they differ, since each change creates a version. Then it reads the result back. It never touches the account's default environment.

## Options

- **Defaults are decisions.** The machine type defaults to the smallest, per the operator's sizing rule, and a larger one is only ever an explicit option. The TTL is a hard ceiling, because Boat cannot tell when an agent goes idle. Stopping an idle sandbox belongs to a profile.
- **Validate when parsing.** Each option is checked when it is read, whether from a flag or from the file, with the same message. A value that becomes a `boat` argument never starts with a dash, which `boat` would read as a flag.

## Testing against Boat

- Creating a sandbox costs money, and setup changes the account. Get the user's go-ahead first, and delete what you created.
- To test behavior, put a fake `boat` on `PATH` that prints recorded JSON.
