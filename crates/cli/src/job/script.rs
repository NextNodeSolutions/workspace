use super::Job;

pub(super) const MAX_PAYLOAD_BYTES: usize = 40 * 1024 * 1024;
pub(super) const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;
pub(super) const PROTOCOL: &str = "workspace-job-v2";
pub(super) const COMPLETE: &str = "complete";
pub(super) const ARTIFACT_FAILED: &str = "artifact-failed";
pub(super) const TOO_LARGE: &str = "too-large";
pub(super) const TRUNCATED: &str = "truncated";
pub(super) const EMPTY_COMMAND: &str = "true";
pub(super) const TRUNCATION_MARKER: &[u8] =
    b"\n[workspace: log truncated; full log remains in the remote record]\n";
const DEFAULT_WORKDIR: &str = "\"$HOME/job\"";
const EXPORT_FAILURE_STATUS: u8 = 125;

pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn workdir(job: &Job) -> String {
    job.workdir
        .as_deref()
        .map_or_else(|| DEFAULT_WORKDIR.to_owned(), quote)
}

pub(super) fn upload(job: &Job) -> String {
    format!(
        r#"set -eu
unset CDPATH
workdir={}
case "$workdir" in /*) ;; *) workdir=$PWD/$workdir ;; esac
parent=${{workdir%/*}}
name=${{workdir##*/}}
refuse_upload() {{ printf '%s\n' 'upload requires a new workdir with an existing parent and no symlink parents' >&2; exit 1; }}
cd -L -- "$parent" 2>/dev/null || refuse_upload
[ "$PWD" = "$(pwd -P)" ] || refuse_upload
mkdir -- "$name" 2>/dev/null || refuse_upload
cd -P -- "$name"
tar --warning=no-unknown-keyword -xzf - -C . 2>/dev/null || {{ printf '%s\n' 'could not extract committed job source' >&2; exit 1; }}"#,
        workdir(job)
    )
}

pub(super) fn setup(job: &Job) -> String {
    phase(
        job,
        "cat > \"$record/setup.sh\" || return\nbash \"$record/setup.sh\" </dev/null >\"$record/log\" 2>&1",
        "",
        MAX_PAYLOAD_BYTES,
    )
}

pub(super) fn command(job: &Job, remaining: usize) -> String {
    let command = job.command.as_deref().unwrap_or(EMPTY_COMMAND);
    let runner = format!(
        "while IFS= read -r -d '' assignment; do export \"$assignment\" || exit {EXPORT_FAILURE_STATUS}; done\nexec bash -c {}",
        quote(command)
    );
    let names = job
        .artifacts
        .iter()
        .map(|name| quote(&format!("./{name}")))
        .collect::<Vec<_>>()
        .join(" ");
    let artifacts = if names.is_empty() {
        String::new()
    } else {
        format!(
            "COPYFILE_DISABLE=1 tar -C \"$workdir\" -cf - -- {names} 2>\"$record/artifacts.error\" | head -c \"$((available + 1))\" > \"$record/artifacts.tar\"\narchive_status=$?"
        )
    };
    phase(
        job,
        &format!("bash -c {} >\"$record/log\" 2>&1", quote(&runner)),
        &artifacts,
        remaining,
    )
}

fn phase(job: &Job, runner: &str, artifacts: &str, limit: usize) -> String {
    let log_limit = MAX_LOG_BYTES.min(limit);
    format!(
        r#"set -u
set -o pipefail
collect_job() {{
workdir={workdir}
record=$(mktemp -d) || return
mkdir -p -- "$workdir" && cd -- "$workdir" || return
workdir=$PWD
{runner}
status=$?
log_size=$(wc -c < "$record/log") || return
log_status={COMPLETE}
if [ "$log_size" -gt {log_limit} ]; then
    log_size={log_limit}
    log_status={TRUNCATED}
fi
available=$(({limit} - log_size))
archive_status=0
: > "$record/artifacts.tar" || return
{artifacts}
archive_size=$(wc -c < "$record/artifacts.tar") || return
collection={COMPLETE}
if [ "$archive_status" -ne 0 ]; then
    collection={ARTIFACT_FAILED}
fi
if [ "$archive_size" -gt "$available" ]; then
    collection={TOO_LARGE}
fi
printf '\n{protocol}\n%s\n%s\n%s\n' "$status" "$collection" "$log_status"
printf '%s' "$record" | base64 | tr -d '\n' || return
printf '\n'
head -c {log_limit} "$record/log" | base64 | tr -d '\n' || return
printf '\n'
if [ "$collection" = {COMPLETE} ]; then
    base64 < "$record/artifacts.tar" | tr -d '\n' || return
fi
printf '\n'
}}
collect_job
"#,
        workdir = workdir(job),
        protocol = PROTOCOL,
    )
}
