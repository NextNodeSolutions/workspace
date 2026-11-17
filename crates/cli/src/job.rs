mod environment;
mod output;
mod process;
mod push;
mod script;
mod vm;

use std::error::Error;
use std::path::{Component, Path, PathBuf};

use clap::Args;
use workspace_core::port::Provider;
use workspace_core::{VmId, lifecycle};

use crate::files;

use crate::ProviderArgs;
use crate::config::{Config, Profiles};

use environment::Environment;
use output::Output;
use script::{MAX_LOG_BYTES, MAX_PAYLOAD_BYTES};

#[derive(Args)]
pub(crate) struct Job {
    /// Name to create or resume; without one the VM is disposable
    name: Option<String>,
    /// Upload the committed HEAD tree scoped to DIR before setup
    #[arg(long, value_name = "DIR", long_help = format!(
        "Upload one pinned HEAD tree scoped to DIR before setup. Local edits, staged additions and untracked files are excluded. Archive attributes are ignored. Unsafe symlinks and submodules are refused. The remote workdir must not exist or have symlink parents. Both the tar and its gzip upload must fit within {} bytes; preparation finishes before creating or resuming a VM. Private source.json records the commit, scope, members and uploaded SHA-256 digest.", push::MAX_PUSH_BYTES
    ))]
    push: Option<PathBuf>,
    /// Local shell script to run before injecting the runtime environment
    #[arg(long, value_name = "PATH")]
    setup: Option<PathBuf>,
    /// Command to run as a shell line after setup succeeds
    #[arg(long, value_name = "COMMAND")]
    command: Option<String>,
    /// Literal remote directory for setup and command [default: $HOME/job]
    #[arg(long, value_name = "PATH")]
    workdir: Option<String>,
    /// Relative report or result path to collect; repeat for several, none collects only logs
    #[arg(long = "artifact", value_name = "PATH", long_help = format!(
        "Relative report or result path to collect; repeat for several. None collects only logs. Each phase captures at most {MAX_LOG_BYTES} log bytes. Setup log, command log and uncompressed artifact tar together must fit within {MAX_PAYLOAD_BYTES} bytes. Truncated logs have an explicit marker; incomplete collection preserves the VM and reports its remote records. Artifacts are returned unchanged; select only paths safe to retain locally."
    ))]
    artifacts: Vec<String>,
    /// Export NAME from the caller's environment only to the command; repeat for several
    #[arg(id = "job_environment", long = "env", value_name = "NAME")]
    environment: Vec<String>,
    /// Keep the VM running after collecting the command result, including disposable VMs
    #[arg(long)]
    keep: bool,
    /// Profile to apply before setup; repeat to apply several in order
    #[arg(long = "profile", value_name = "PROFILE")]
    profiles: Vec<String>,
    #[command(flatten)]
    provider: ProviderArgs,
    #[command(flatten)]
    profile_options: Profiles,
}

#[derive(Debug, thiserror::Error)]
enum JobError {
    #[error("a job needs --setup PATH or --command COMMAND")]
    NoInput,
    #[error("--artifact paths must be nonempty relative paths without parent components")]
    ArtifactPath,
    #[error("job shell scripts and workdir must contain no NUL bytes")]
    ShellNul,
    #[error("--workdir must not be empty")]
    EmptyWorkdir,
    #[error(
        "setup failed with exit status {status} on {vm}; command skipped, VM and record kept; run `{next}` when finished inspecting it"
    )]
    Setup { vm: VmId, status: u8, next: String },
    #[error(
        "created {vm} on {provider} without a record; it is still running; use the provider CLI to stop it, as described in the job documentation"
    )]
    Unrecorded {
        vm: VmId,
        provider: String,
        #[source]
        cause: workspace_core::Error,
    },
    #[error("job failed on {vm}; VM and record kept; run `{next}` when finished inspecting it")]
    Preserved {
        vm: VmId,
        next: String,
        #[source]
        cause: Box<dyn Error>,
    },
    #[error("job result collected, but cleanup failed for {vm}; run `{next}`")]
    Cleanup {
        vm: VmId,
        next: String,
        #[source]
        cause: workspace_core::Error,
    },
}

pub(crate) fn run(job: &Job, file: Config) -> Result<u8, Box<dyn Error>> {
    let environment = Environment::inherit(&job.environment)?;
    run_masked(job, file, &environment).map_err(|error| environment.error(error.as_ref()))
}

fn run_masked(job: &Job, file: Config, environment: &Environment) -> Result<u8, Box<dyn Error>> {
    validate(job)?;
    let setup = prepare_script(job, environment)?;
    let upload = push::archive(job.push.as_deref(), environment)?;
    let directory = output::directory()?;
    if let Some(upload) = &upload {
        upload.save(&directory)?;
    }
    let running = vm::start(job, file)?;
    let vm = running.vm.clone();
    eprintln!(
        "{}",
        environment.message(&format!(
            "{vm} is running; `{}` puts it away. Job records: {}",
            running.recovery(),
            directory.display()
        ))
    );
    let result = collect(
        running.provider.as_ref(),
        &vm,
        job,
        &setup,
        upload.as_ref().map(|upload| upload.bytes.as_slice()),
        environment,
        &directory,
    );
    let status = result.map_err(|error| JobError::Preserved {
        vm: vm.clone(),
        next: running.recovery(),
        cause: error,
    })?;
    if let Some(status) = status.setup_failure {
        return Err(JobError::Setup {
            vm,
            status,
            next: running.recovery(),
        }
        .into());
    }
    if !job.keep {
        running.put_away()?;
    }
    println!("{}", directory.display());
    Ok(status.command)
}

fn prepare_script(job: &Job, environment: &Environment) -> Result<String, Box<dyn Error>> {
    let setup = job
        .setup
        .as_ref()
        .map(|path| std::fs::read_to_string(path).map_err(files::io_error("read", path)))
        .transpose()?;
    let literals = setup
        .iter()
        .chain(job.command.iter())
        .chain(job.workdir.iter())
        .chain(job.artifacts.iter());
    for literal in literals {
        environment.reject_inline(literal)?;
        if literal.contains('\0') {
            return Err(JobError::ShellNul.into());
        }
    }
    Ok(setup.unwrap_or_else(|| script::EMPTY_COMMAND.to_owned()))
}

struct Collected {
    setup_failure: Option<u8>,
    command: u8,
}

fn collect(
    provider: &dyn Provider,
    vm: &VmId,
    job: &Job,
    setup: &str,
    upload: Option<&[u8]>,
    environment: &Environment,
    directory: &Path,
) -> Result<Collected, Box<dyn Error>> {
    if let Some(archive) = upload {
        lifecycle::execute(provider, vm, &script::upload(job), archive)?;
    }
    let setup_output = lifecycle::execute(provider, vm, &script::setup(job), setup.as_bytes())?;
    let setup_output = Output::parse(&setup_output, MAX_PAYLOAD_BYTES)?;
    setup_output.save(directory, "setup", environment)?;
    if setup_output.status != 0 {
        return Ok(Collected {
            setup_failure: Some(setup_output.status),
            command: 0,
        });
    }
    let remaining = MAX_PAYLOAD_BYTES - setup_output.log.len();
    let output = lifecycle::execute(
        provider,
        vm,
        &script::command(job, remaining),
        &environment.stdin(),
    )?;
    let output = Output::parse(&output, remaining)?;
    output.save(directory, "command", environment)?;
    Ok(Collected {
        setup_failure: None,
        command: output.status,
    })
}

fn validate(job: &Job) -> Result<(), JobError> {
    if job.setup.is_none() && job.command.is_none() {
        return Err(JobError::NoInput);
    }
    if job.workdir.as_deref() == Some("") {
        return Err(JobError::EmptyWorkdir);
    }
    for artifact in &job.artifacts {
        if artifact.is_empty()
            || artifact.contains('\0')
            || Path::new(artifact).components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(JobError::ArtifactPath);
        }
    }
    Ok(())
}
