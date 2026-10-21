# Working on workspace

These are the rules for changing this repository, for people and coding agents alike. Each crate has its own `AGENTS.md` with the rules that apply only there. To learn how the project works, read the README, then `docs/`.

## What the project guarantees

These come first. A change that weakens one is wrong, even when it compiles.

1. **A running VM is never lost.** VMs cost money. The record is written as soon as a provider returns an id, before anything waits on the VM, and a provider returns the id as early as it can. Nothing stops or deletes a VM because a step failed. The error names the VM, and the user decides.
2. **Every command can be repeated.** Setup, profiles, stop and remove are safe to run again. Something already done counts as success, such as stopping a stopped VM or removing a deleted one.
3. **A record never disappears before its VM.** `stop` keeps the record. `remove` deletes it only after the provider confirms the VM is gone.
4. **Secrets travel on stdin.** They never appear in a script, a command argument, a log or an error message.
5. **A VM gets the least it needs from the account.** Provider accounts hold credentials. An adapter passes a VM only what it needs, and checks that before creating one.
6. **Shared account resources stay untouched.** Setup creates and owns what it needs. It never changes what other tools or people on the account rely on, such as a default environment.

## Architecture rules

The project is built as ports and adapters. `docs/architecture.md` shows the pieces and the flows.

- **Dependencies point inward.** The core depends on no crate of this repository. An adapter depends on the core only, never on another adapter. Only the cli depends on adapters. As a result, the compiler guarantees that adding or changing a provider never touches the core.
- **The core does no IO.** It touches no files, processes or network, reads no environment variables and prints nothing. The logic stays provider-agnostic and easy to reason about, and IO stays at the edges.
- **The cli is the only composition root.** Only the cli parses arguments, reads the config file and the environment, picks adapters by name, keeps the records and prints. Its commands stay thin.
- **Provider details stay in the adapter.** The core holds what every provider needs, such as a VM name. Anything specific, such as size, lifetime, region or credentials, is an adapter option.
- **Failures cross into the core as opaque causes.** An adapter turns each failure of its provider into its own error type, with a message that says what failed and what to do. The core wraps that error without inspecting or downcasting it. No provider type appears in a core signature, and nothing a provider prints reaches the terminal raw.
- **Each adapter owns its options.** One struct per adapter gives both its flags and its config table, holds the defaults and validates the values. The precedence is always flag, then config file, then default. An unknown config key is an error.
- **External calls are bounded.** A call that should answer quickly gets a time limit. Output read from another process gets a size limit. Only work that can rightly run long, such as a provisioning script, goes without one.
- **Decide on codes and state, never on wording.** When an external tool fails, branch on its error code, or read the state back. Messages change between versions, codes do not.

## Code rules

- **Lints.** The workspace turns on pedantic clippy and forbids `unsafe`. Code passes with no warning. Silence a lint only with `#[expect(lint, reason = "...")]`, and give the real reason.
- **Size.** `clippy.toml` caps function length and complexity. Keep each file under 250 lines and nesting at 2 levels or fewer. When a limit gets close, split by responsibility, not by line count.
- **Literals.** Every number or string with meaning gets a named constant, whose name says what it is.
- **Toolchain.** Stay within the `rust-version` of the workspace manifest.
- **Errors.** Errors are `thiserror` types, local to the crate that raises them. A message says what failed and, when the user can act, what to do. An error that leaves something behind says so, for example that the VM is still running or that its record is kept.
- **Panics.** Only for invariants the code itself guarantees, through an `expect` whose message states the invariant. Never for input, and never for another program's output.
- **Comments.** Write none, so no comment can drift from the code. The exceptions are:
  - one short sentence for a choice the code cannot show, such as an invariant, an order constraint, an outside requirement, or a workaround with its reason, unless an `AGENTS.md` or the docs already state it;
  - the `# Errors` and `# Panics` sections that clippy requires on public functions;
  - the clap help text, which users read in `--help`.

  Everything else goes in names, in the docs or in the commit message.
- **Tests.** Add them only when the task asks for them.
- **Language.** Code, comments, commits and docs are in English.

## Checks

Run these before every commit. All must pass with no warning:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Commits

- One change per commit, in [Conventional Commits](https://www.conventionalcommits.org) form, such as `feat(provider-boat): ...` or `fix(cli): ...`.
- Every commit builds and passes the checks on its own. To check one alone, export it with `git archive` and run the checks on the export, in a fresh target directory.
- No model identifier goes in a commit, a pull request or the code.

## Documentation

- **The docs describe the intended behavior.** That covers `docs/`, the README and the `AGENTS.md` files. When a change makes them wrong, do not just rewrite them to match. First check that the change is expected: the task asks for it, or the user confirms it.
  - If it is expected, update the docs in the same commit as the change.
  - If it is not, the docs win. Bring the code back in line with them, or stop and ask.
- Docs explain concepts, flows, states and the reasons for choices. They never list files, copy constants from the code, or pin versions or dates, since those go stale. Point to the code or to `--help` for values.
- Diagrams are Mermaid. Render each one before committing to check that it parses.
- The `AGENTS.md` files follow the same rules: each rule explains why, and none of them indexes the code.

## Working against real providers

- Starting a VM costs money, and setup changes the provider account. Get the user's go-ahead before either, and delete what you created afterwards.
- To test behavior, use a fake provider CLI on `PATH` whenever you can.
- Never print, log or commit credentials.

## Adding a provider or a profile

1. Create a crate that depends only on the core, and implement `Provider` or `Profile`.
2. Add it as a dependency of the cli, and register it by name in the cli's adapter registry.
3. If it takes options, add its options struct to the cli's provider options. That one field gives the adapter both its flags and its `[providers.<name>]` table.
4. Describe it in a page under `docs/providers/`, and write its own `AGENTS.md`.

The core does not change. If it seems to need a change, a port is missing a concept, so discuss that before you code.
