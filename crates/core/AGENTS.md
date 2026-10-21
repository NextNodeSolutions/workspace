# Core rules

The core is the domain: the life of a VM, and the ports that adapters implement. Everything here must hold for every provider, present and future.

- **No IO, and no reason to need any.** The core reads no files, no environment variables, spawns no processes, opens no network and prints nothing. No dependency may bring that in, or tie the core to a provider, a command line or a config format.
- **A port is a contract for every adapter.** `docs/architecture.md` states what each method must do and what makes it safe to repeat. The method's `# Errors` section says what may still exist after a failure. A change to a port changes every adapter and `docs/architecture.md` in the same commit.
- **Order is the invariant.** In the lifecycle functions, the record is written right after the provider returns an id, before any wait. A record is forgotten only after the provider confirms the deletion. A failure path never calls stop or remove: the VM is left as it is, and the error names it.
- **One error per step.** Each lifecycle step maps its adapter's cause to one variant of the core's error, unless the failure's reason says more: a failure that needs setup, or a VM that is gone, has its own variant whatever the step. The message tells the user what is left, such as whether the VM may still run or whether its record is kept. The cause stays attached, uninspected.
- **Specs stay minimal.** A VM spec holds only what every provider needs. A setting that only some providers understand is an option of those adapters, not a spec field.
- **Check values where they enter.** A value that enters the core, such as a VM name, is validated when it is built, so the rest of the core can trust it.
