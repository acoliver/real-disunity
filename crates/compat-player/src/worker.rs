use crate::{Session, playback::Playback};
use compat_kit::{
    RunRequest, WorkerReport,
    model::{Diagnostic, Status, Teardown},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn run_worker() -> Result<(), String> {
    let root = variable("COMPAT_ROOT")?;
    let request_path = variable("COMPAT_REQUEST")?;
    let response_path = variable("COMPAT_RESPONSE")?;
    let request: RunRequest =
        serde_json::from_slice(&fs::read(request_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let report = run_request(&root, &request);
    fs::write(
        response_path,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
fn variable(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}"))
}

pub fn run_request(root: &Path, request: &RunRequest) -> WorkerReport {
    let mut report = WorkerReport {
        schema_version: 1,
        evidence_label: request.metadata["evidence_label"]
            .as_str()
            .unwrap_or("original")
            .into(),
        observations: Vec::new(),
        captures: Vec::new(),
        diagnostics: Vec::new(),
        teardown: Teardown {
            completed: false,
            native_worlds: 0,
            managed_roots: 0,
            pending_readbacks: 0,
        },
    };
    let mut session = None;
    if let Err(error) = execute(root, request, &mut report, &mut session) {
        diagnostic(&mut report, error);
    }
    finish(&mut report, session);
    report
}
fn finish(report: &mut WorkerReport, session: Option<Session>) {
    if let Some(mut session) = session {
        if let Err(error) = session.ensure_no_callback_failures() {
            diagnostic(report, error);
        }
        match teardown(&mut session) {
            Ok((worlds, roots)) => {
                report.teardown.completed = true;
                report.teardown.native_worlds = worlds;
                report.teardown.managed_roots = roots;
            }
            Err(error) => diagnostic(report, format!("teardown failed: {error}")),
        }
    }
}
fn teardown(session: &mut Session) -> Result<(u64, u64), String> {
    session.unload()?;
    let (worlds, _objects) = session.statistics()?;
    Ok((worlds, session.managed_roots()?))
}
fn execute(
    root: &Path,
    request: &RunRequest,
    report: &mut WorkerReport,
    session: &mut Option<Session>,
) -> Result<(), String> {
    if request.schema_version != 1 {
        return Err("unsupported run schema version".into());
    }
    crate::strict::admit(request)?;
    crate::interactive::Config::admit(request)?;
    let mut playback = Playback::new(root, request, report, session)?;
    if request.desktop {
        return crate::desktop::run(playback);
    }
    for action in &request.timeline {
        playback.action(action)?;
    }
    Ok(())
}
fn diagnostic(report: &mut WorkerReport, message: String) {
    report.diagnostics.push(Diagnostic {
        status: Status::Error,
        message,
    });
}

#[cfg(test)]
mod callback_tests {
    use super::*;

    #[test]
    fn actual_worker_finalization_rejects_unconsumed_callback_failures() {
        // CoreCLR retains its configured provider for the lifetime of the process.
        if std::env::var_os("DISUNITY_WORKER_CALLBACK_CHILD").is_none() {
            let outputs = std::env::var_os("DISUNITY_TEST_OUTPUT_DIR")
                .map_or_else(
                    || PathBuf::from(".work/validation-handshake"),
                    PathBuf::from,
                )
                .join(format!("worker-child-{}", std::process::id()));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "worker::callback_tests::actual_worker_finalization_rejects_unconsumed_callback_failures",
                    "--nocapture",
                ])
                .env("DISUNITY_WORKER_CALLBACK_CHILD", "1")
                .env("DISUNITY_TEST_OUTPUT_DIR", outputs)
                .status()
                .unwrap();
            assert!(status.success(), "callback worker child failed: {status}");
            return;
        }
        check_finalization();
    }

    fn check_finalization() {
        let session = crate::session::callback_test::session();
        let mut report = WorkerReport {
            schema_version: 1,
            evidence_label: "authored-native-callback".into(),
            observations: Vec::new(),
            captures: Vec::new(),
            diagnostics: Vec::new(),
            teardown: Teardown {
                completed: false,
                native_worlds: 1,
                managed_roots: 1,
                pending_readbacks: 0,
            },
        };
        finish(&mut report, Some(session));
        assert!(report.teardown.completed);
        assert_eq!(report.teardown.native_worlds, 0);
        assert_eq!(report.teardown.managed_roots, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.status == Status::Error
                    && diagnostic.message.contains("callback sentinel Update")
                    && diagnostic.message.contains("System.ArgumentException")),
            "callback failures silently passed: {report:?}"
        );
    }
}
