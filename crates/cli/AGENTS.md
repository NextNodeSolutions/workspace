# CLI rules

The cli is the composition root. It is the only crate that knows every adapter, and it does all the IO that no provider does.

- **Commands stay thin.** A command reads its arguments and the config, builds the adapters, calls one core function and prints the result. Logic that holds for every provider belongs in the core, and logic for one provider belongs in its adapter.
- **stdout carries results only.** A command prints its result, such as a VM id or the setup summary, on stdout, so scripts can capture it. Errors go to stderr: one line for the error, then one line per cause.
- **An adapter appears in one place.** An adapter is registered by name in the adapter registry. Its options join the provider options group, which gives both the flags and the `[providers.<name>]` config table. Nothing else in the cli names a specific provider.
- **Provider options come last in a command.** clap applies a flattened group's help heading to every argument after it. Put a command's other arguments before the provider options, or they show under the provider's heading.
- **Config precedence is flag, then file, then default.** Defaults live in each adapter, never here. Unknown keys and tables are errors, so a typo cannot pass silently. The default file is optional, but a file named with `--config` must exist. Every command loads the config, `stop` and `remove` included, since they build providers the same way.
- **Only `setup` writes the config file, and only its `provider` key.** People edit the file by hand, so the rest of it, comments included, stays as it was. The provider becomes the default only once its setup succeeds, so a failed setup leaves the file unchanged.
- **Records are an on-disk contract.** Users rely on them to find VMs that cost money. A change to the format must keep existing records readable. A record is written under a temporary name and then renamed, so it is never half written. A VM id comes from a provider, so it is checked before it becomes a file name.
- **A name means one VM.** Commands take a VM's name in place of its id, so `start` resumes the VM recorded under a name instead of creating a second one, on the provider its record names. When several records share a name, a lookup fails and lists their ids instead of picking one, since picking the wrong VM could stop or delete it.
- **Paths follow XDG.** Data and config go under the XDG base directories, with the fallbacks the spec defines.
