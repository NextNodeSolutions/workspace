# Configuration

Providers take options, such as a machine type. You set an option with a flag, in the config file, or not at all.

## Which value wins

```mermaid
flowchart LR
    start([an option]) --> flag{flag given?}
    flag -- yes --> useFlag[use the flag]
    flag -- no --> file{key in the config file?}
    file -- yes --> useFile[use the file]
    file -- no --> useDefault[use the provider's default]
```

A flag lasts for one command. The file holds what you want every time.

## The config file

| Situation | File read |
| --- | --- |
| `--config <path>` given | That file. It must exist. |
| `XDG_CONFIG_HOME` set | `$XDG_CONFIG_HOME/workspace/config.toml` |
| Otherwise | `~/.config/workspace/config.toml` |

The default file is optional. Without it, every option keeps its default.

Each provider reads its own table under `[providers]`, named after the provider:

```toml
[providers.boat]
type = "small"
```

Each provider's page lists its keys, and `workspace boot --help` lists the matching flags.

## Mistakes stop the command

An unknown key, an unknown table or a bad value is an error, and the error points at the line. So a typo never goes unnoticed.

Every command reads the file, `close` and `remove` included, because they build the provider the same way. A broken file therefore blocks them too until you fix it.
