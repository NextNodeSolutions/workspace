use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentError {
    #[error("cannot run `{program}` on this machine: install it and put it on PATH")]
    Spawn {
        program: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("lost the pipe to `{program}`")]
    Pipe {
        program: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("`{program}` did not finish within {seconds} s, so it was killed")]
    Stalled { program: &'static str, seconds: u64 },

    #[error("`{program}` printed more than {limit_mib} MiB")]
    TooMuchOutput {
        program: &'static str,
        limit_mib: u64,
    },

    #[error("`{program}` failed{}", on_new_line(.stderr))]
    Failed {
        program: &'static str,
        stderr: String,
    },

    #[error("`infisical export` printed JSON this profile cannot read")]
    Unreadable(#[source] serde_json::Error),

    #[error(
        "the Infisical project `{project}` has no secret named {missing} in its `{environment}` environment: add it there, or drop it from the profile's secrets"
    )]
    MissingSecrets {
        project: String,
        environment: String,
        missing: String,
    },

    #[error(
        "the `{profile}` profile exports secrets but names no Infisical project: set `infisical_project` under [profiles.{profile}] in the config file, or pass --{profile}-infisical-project"
    )]
    NoProject { profile: &'static str },

    #[error("cannot locate the home directory: set HOME")]
    NoHome,

    #[error(
        "Pi is not installed on this machine, so there is no version to match: pass --pi-version, or set `version` under [profiles.pi]"
    )]
    NoLocalPi,

    #[error("`pi --version` on this machine printed `{0}`, which is not a version")]
    LocalVersion(String),

    #[error("none of the files to mirror exist under {}", .0.display())]
    NothingToMirror(PathBuf),

    #[error("cannot read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} does not hold a JSON object", .0.display())]
    NotObject(PathBuf),

    #[error(
        "this machine's environment has no `{name}`: export it, or drop `env:{name}` from the profile's secrets"
    )]
    MissingEnvironment { name: String },
}

fn on_new_line(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!(":\n{text}")
}
