use crate::reporting::{FailureCause, FailureKind};
use crate::{
    files::{self, Result, io},
    model::{CommandSpec, Limits, RunRequest, WorkerReport},
};
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct RunControl {
    cancelled: Arc<AtomicBool>,
}
impl RunControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
#[derive(Debug)]
pub struct ProcessOutcome {
    pub success: bool,
    pub exit: String,
    pub interrupted: Option<String>,
    pub truncated_logs: bool,
    pub failure_cause: Option<FailureCause>,
}
pub struct Execution<'a> {
    pub cwd: &'a Path,
    pub env: &'a BTreeMap<String, String>,
    pub logs: &'a Path,
    pub artifacts: Option<&'a Path>,
    pub limits: &'a Limits,
}
fn drain(mut stream: impl Read, path: &Path, cap: u64) -> std::io::Result<bool> {
    let mut file = fs::File::create(path)?;
    let mut written = 0;
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let available = usize::try_from(cap.saturating_sub(written))
            .unwrap_or(usize::MAX)
            .min(count);
        file.write_all(&buffer[..available])?;
        written += available as u64;
        truncated |= available < count;
    }
    Ok(truncated)
}
fn wait(
    child: &mut Child,
    options: &Execution<'_>,
    control: &RunControl,
) -> (Option<ExitStatus>, Option<FailureCause>) {
    let start = Instant::now();
    let mut budget_checked = Instant::now();
    loop {
        if budget_checked.elapsed() >= Duration::from_secs(1) {
            if let Err(error) = budget(options) {
                return (
                    None,
                    Some(FailureCause::new(
                        FailureKind::Infrastructure,
                        "supervision",
                        error,
                    )),
                );
            }
            budget_checked = Instant::now();
        }
        if control.is_cancelled() {
            return (
                None,
                Some(FailureCause::new(
                    FailureKind::Cancelled,
                    "supervision",
                    "cancelled",
                )),
            );
        }
        if start.elapsed() >= Duration::from_millis(options.limits.deadline_ms) {
            return (
                None,
                Some(FailureCause::new(
                    FailureKind::Deadline,
                    "supervision",
                    "worker deadline exceeded",
                )),
            );
        }
        if let Some(path) = options.artifacts
            && let Err(error) = files::live_tree_files(path, options.limits.artifact_bytes, 4096)
        {
            return (
                None,
                Some(FailureCause::new(
                    FailureKind::Infrastructure,
                    "supervision-artifacts",
                    error.to_string(),
                )),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => return (Some(status), None),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => return (None, Some(wait_error(&error))),
        }
    }
}
fn budget(options: &Execution<'_>) -> Result<()> {
    if let Some(root) = options.env.get("COMPAT_ROOT") {
        files::check_budget(Path::new(root))?;
    }
    Ok(())
}
pub fn execute(
    command: &CommandSpec,
    options: &Execution<'_>,
    control: &RunControl,
) -> std::result::Result<ProcessOutcome, FailureCause> {
    execute_inner(command, options, control)
        .map_err(|message| FailureCause::new(FailureKind::Infrastructure, "process", message))
}
fn execute_inner(
    command: &CommandSpec,
    options: &Execution<'_>,
    control: &RunControl,
) -> Result<ProcessOutcome> {
    options.limits.validate()?;
    budget(options)?;
    if control.is_cancelled() {
        return Ok(ProcessOutcome {
            success: false,
            exit: "not started".into(),
            interrupted: Some("cancelled".into()),
            truncated_logs: false,
            failure_cause: Some(FailureCause::new(
                FailureKind::Cancelled,
                "supervision",
                "cancelled",
            )),
        });
    }
    fs::create_dir_all(options.logs).map_err(io)?;
    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args)
        .current_dir(options.cwd)
        .envs(options.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("worker unavailable: {}: {e}", command.program))?;
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.id()).map_err(io)?)
        .ok_or("invalid process id")?;
    let stdout = child.stdout.take().ok_or("stdout pipe missing")?;
    let stderr = child.stderr.take().ok_or("stderr pipe missing")?;
    let out_path = options.logs.join("stdout.log");
    let err_path = options.logs.join("stderr.log");
    let log_cap = options.limits.log_bytes;
    let out = thread::spawn(move || drain(stdout, &out_path, log_cap));
    let err = thread::spawn(move || drain(stderr, &err_path, log_cap));
    let (status, interrupted) = wait(&mut child, options, control);
    // Kill the owned group after leader exit too, releasing inherited pipes and children.
    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    let reaped = child.wait().map_err(io)?;
    let status = status.unwrap_or(reaped);
    let out_truncated = out
        .join()
        .map_err(|_| "stdout reader panicked")?
        .map_err(io)?;
    let err_truncated = err
        .join()
        .map_err(|_| "stderr reader panicked")?
        .map_err(io)?;
    Ok(ProcessOutcome {
        success: status.success(),
        exit: status.to_string(),
        interrupted: interrupted.as_ref().map(|cause| cause.message.clone()),
        failure_cause: interrupted,
        truncated_logs: out_truncated || err_truncated,
    })
}
pub trait RuntimeClient {
    fn execute(&self, request: &RunRequest, control: &RunControl) -> Result<WorkerReport>;
}
pub struct ProcessRuntimeClient<'a> {
    pub command: &'a CommandSpec,
    pub root: &'a Path,
    pub run_dir: &'a Path,
    pub limits: &'a Limits,
}
impl RuntimeClient for ProcessRuntimeClient<'_> {
    fn execute(&self, request: &RunRequest, control: &RunControl) -> Result<WorkerReport> {
        self.execute_report(request, control).map_err(String::from)
    }
}
impl ProcessRuntimeClient<'_> {
    pub fn execute_report(
        &self,
        request: &RunRequest,
        control: &RunControl,
    ) -> std::result::Result<WorkerReport, FailureCause> {
        self.worker_report(request, control).map_err(|error| {
            let path = self.run_dir.join("process.json");
            files::read_json::<serde_json::Value>(&path)
                .ok()
                .and_then(|v| serde_json::from_value(v["failure_cause"].clone()).ok())
                .unwrap_or_else(|| FailureCause::new(FailureKind::Infrastructure, "worker", error))
        })
    }
    fn worker_report(&self, request: &RunRequest, control: &RunControl) -> Result<WorkerReport> {
        let request_path = self.run_dir.join("request.json");
        let response_path = self.run_dir.join("response.json");
        files::write_json(&request_path, request)?;
        let env = BTreeMap::from([
            (
                "COMPAT_REQUEST".into(),
                request_path.to_string_lossy().into_owned(),
            ),
            (
                "COMPAT_RESPONSE".into(),
                response_path.to_string_lossy().into_owned(),
            ),
            (
                "COMPAT_ARTIFACTS".into(),
                request.artifact_root.to_string_lossy().into_owned(),
            ),
            (
                "COMPAT_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
        ]);
        let options = Execution {
            cwd: self.root,
            env: &env,
            logs: self.run_dir,
            artifacts: Some(self.run_dir),
            limits: self.limits,
        };
        let outcome = execute(self.command, &options, control)?;
        files::write_json(
            &self.run_dir.join("process.json"),
            &serde_json::json!({"exit":outcome.exit,"interrupted":outcome.interrupted,"truncated_logs":outcome.truncated_logs,"failure_cause":outcome.failure_cause}),
        )?;
        if let Err(error) = files::tree_files(self.run_dir, self.limits.artifact_bytes, 4096) {
            files::discard_worker_outputs(self.run_dir)?;
            return Err(format!("final artifact scan: {error}"));
        }
        if let Some(reason) = outcome.interrupted {
            return Err(reason);
        }
        if !outcome.success {
            return Err(format!("worker exit failure: {}", outcome.exit));
        }
        files::read_json(&response_path).map_err(|e| format!("worker response: {e}"))
    }
}

fn wait_error(error: &std::io::Error) -> FailureCause {
    FailureCause::new(
        FailureKind::Infrastructure,
        "wait",
        format!("worker wait error: {error}"),
    )
}
