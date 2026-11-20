use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const SANDBOX: &str = "bx_regression1";
const ENVIRONMENT: &str = "workspace-test";
const RAW_MARKER: &str = "raw-provider-text-marker";
const WAIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(20);

static RUNS: AtomicU32 = AtomicU32::new(0);

/// Where the fake `boat` prints the failure that `new` reports after the id.
#[derive(Clone, Copy)]
enum Refusal {
    Stdout,
    Stderr,
    AfterTheTestReleasesIt,
}

struct Fixture {
    root: PathBuf,
}

struct Run {
    child: Child,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
}

#[test]
fn create_reports_a_post_id_refusal_from_boat_new() {
    let fixture = Fixture::new(Refusal::Stdout);
    let mut run = fixture.start();
    let warning = run.wait_for_warning();
    assert!(
        warning.contains("`boat new` reported a problem after assigning sandbox `bx_regression1`"),
        "{warning}"
    );
    assert!(warning.contains("api_key_action_forbidden"), "{warning}");
    assert!(warning.contains("HTTP 403"), "{warning}");
    assert!(!warning.contains(RAW_MARKER), "{warning}");
    fixture.release("ready");
    let (status, stdout, stderr) = run.finish();
    assert!(status.success(), "{stderr:?}");
    assert_eq!(stdout.trim(), SANDBOX);
}

#[test]
fn create_reports_stderr_refusal_while_boat_new_stdout_remains_open() {
    let fixture = Fixture::new(Refusal::Stderr);
    let mut run = fixture.start();
    let warning = run.wait_for_warning();
    assert!(warning.contains("api_key_action_forbidden"), "{warning}");
    fixture.release("ready");
    let (status, _, stderr) = run.finish();
    assert!(status.success(), "{stderr:?}");
    assert!(!fixture.root.join("new-finished").exists());
    fixture.release("new-exit");
}

#[test]
fn create_returns_the_id_while_boat_new_still_runs() {
    let fixture = Fixture::new(Refusal::AfterTheTestReleasesIt);
    let run = fixture.start();
    let record = fixture.wait_for_record();
    assert_eq!(
        record.lines().collect::<Vec<_>>(),
        ["provider=boat", "name=dev"]
    );
    assert!(
        !fixture.root.join("new-finished").exists(),
        "`boat new` had already finished before the id was recorded"
    );
    fixture.release("ready");
    let (status, stdout, stderr) = run.finish();
    assert!(status.success(), "{stderr:?}");
    assert_eq!(stdout.trim(), SANDBOX);
    fixture.release("new-exit");
}

impl Fixture {
    fn new(refusal: Refusal) -> Self {
        let run = RUNS.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!("workspace-create-{}-{run}", std::process::id()));
        let fixture = Self { root };
        fs::create_dir_all(fixture.root.join("bin")).expect("create the fixture directory");
        fixture.write_config();
        fixture.write_boat(refusal);
        fixture
    }

    fn write_config(&self) {
        let config =
            format!("provider = \"boat\"\n\n[providers.boat]\nenvironment = \"{ENVIRONMENT}\"\n");
        fs::write(self.root.join("config.toml"), config).expect("write the config file");
    }

    fn write_boat(&self, refusal: Refusal) {
        let script = format!(
            "#!/bin/sh\ndir=\"{root}\"\ncase \"$1\" in\nenv)\n{listing}\n  ;;\nnew)\n  printf '%s\\n' '{{\"event\":\"created\",\"id\":\"{SANDBOX}\"}}'\n{failure}\n  ;;\ninfo)\n  {wait_ready}\n  printf '%s\\n' '{{\"sandbox\":{{\"state\":\"ready\"}}}}'\n  ;;\n*)\n  exit 1\n  ;;\nesac\n",
            root = self.root.display(),
            listing = LISTING,
            failure = refusal.script(),
            wait_ready = wait_for("ready"),
        );
        let path = self.boat();
        fs::write(&path, script).expect("write the fake boat");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("make the fake boat runnable");
    }

    fn boat(&self) -> PathBuf {
        self.root.join("bin").join("boat")
    }

    fn start(&self) -> Run {
        let paths = env::var("PATH").unwrap_or_default();
        let path = format!("{}:{paths}", self.root.join("bin").display());
        let mut child = Command::new(env!("CARGO_BIN_EXE_workspace"))
            .arg("--config")
            .arg(self.root.join("config.toml"))
            .args(["start", "dev", "-d"])
            .env("PATH", path)
            .env("HOME", &self.root)
            .env("XDG_DATA_HOME", self.root.join("data"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run the workspace binary");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        Run {
            child,
            stdout: lines(stdout),
            stderr: lines(stderr),
        }
    }

    fn wait_for_record(&self) -> String {
        let path = self.record();
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if let Ok(text) = fs::read_to_string(&path) {
                return text;
            }
            thread::sleep(POLL);
        }
        panic!("no record appeared at {} within {WAIT:?}", path.display());
    }

    fn record(&self) -> PathBuf {
        self.root.join("data").join("workspace/vms").join(SANDBOX)
    }

    fn release(&self, marker: &str) {
        fs::write(self.root.join(marker), "").expect("write a release marker");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(self.root.join("ready"), "");
        let _ = fs::write(self.root.join("new-exit"), "");
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Refusal {
    fn script(self) -> String {
        let event = format!(
            "'{{\"event\":\"error\",\"code\":\"api_key_action_forbidden\",\"status\":403,\"error\":\"{RAW_MARKER}\"}}'"
        );
        match self {
            Self::Stdout => format!("  printf '%s\\n' {event}\n  exit 1"),
            Self::Stderr => format!(
                "  printf '%s\\n' {event} >&2\n  exec 2>&-\n  {}\n  : > \"$dir/new-finished\"\n  exit 1",
                wait_for("new-exit")
            ),
            Self::AfterTheTestReleasesIt => format!(
                "  {}\n  : > \"$dir/new-finished\"\n  exit 0",
                wait_for("new-exit")
            ),
        }
    }
}

impl Run {
    fn wait_for_warning(&mut self) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.stderr.recv_timeout(remaining) {
                Ok(line) if line.starts_with("warning: ") => return line,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    panic!("no warning within {WAIT:?}");
                }
            }
        }
    }

    fn finish(mut self) -> (std::process::ExitStatus, String, Vec<String>) {
        let status = self.child.wait().expect("wait for the workspace process");
        let stdout = self.stdout.iter().collect::<Vec<String>>().join("\n");
        let stderr = self.stderr.iter().collect::<Vec<String>>();
        (status, stdout, stderr)
    }
}

fn wait_for(marker: &str) -> String {
    format!(
        "count=0\n  while [ ! -f \"$dir/{marker}\" ] && [ \"$count\" -lt 500 ]; do sleep 0.02; count=$((count+1)); done"
    )
}

fn lines(pipe: impl Read + Send + 'static) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { return };
            if sender.send(line).is_err() {
                return;
            }
        }
    });
    receiver
}

const LISTING: &str = "  printf '%s\\n' '{\"environments\":[{\"name\":\"workspace-test\",\"isDefault\":false,\"safeForThirdParties\":false,\"passGithub\":false,\"passSecrets\":false,\"passAgentsCredentials\":false,\"passSandboxCredentials\":true,\"effective\":{\"passGithub\":false,\"passSecrets\":false,\"passAgentsCredentials\":false,\"passSandboxCredentials\":true}}]}'";
