use crate::reporting::{self, FailureCause, FailureKind, Prerequisite};
use crate::{
    evidence,
    files::{self, Result, io},
    fixture,
    model::{Fixture, RunRecord, RunRequest, Status, WorkerReport},
    process::{ProcessRuntimeClient, RunControl},
};
use std::{path::Path, time::Instant};

pub fn run(
    root: &Path,
    fixture: &Fixture,
    profile: Option<&str>,
    desktop: bool,
    control: &RunControl,
) -> Result<RunRecord> {
    files::check_budget(root)?;
    let owned = crate::storage::allocate(root)?;
    let run_dir = owned.path.clone();
    let run_id = run_dir
        .file_name()
        .ok_or("run id missing")?
        .to_string_lossy()
        .into_owned();
    let started = Instant::now();
    let mut record = RunRecord {
        schema: crate::model::TckRunSchema::V1,
        schema_version: 1,
        run_id,
        fixture: fixture.name.clone(),
        profile: profile.unwrap_or(&fixture.default_profile).into(),
        evidence_label: fixture.evidence_label.clone(),
        status: Status::Inconclusive,
        run_dir: run_dir.strip_prefix(root).map_err(io)?.to_path_buf(),
        started_ms: files::now_ms(),
        elapsed_ms: 0,
        messages: Vec::new(),
        assertions: Vec::new(),
        captures: Vec::new(),
        worker: None,
        reporting: crate::identity::context(fixture, desktop)?,
    };
    let result = execute_run(root, fixture, desktop, &run_dir, control, &mut record);
    if let Err(message) = result {
        record.status = Status::Error;
        record.reporting.failure_causes.push(FailureCause::new(
            FailureKind::Infrastructure,
            "run",
            message.clone(),
        ));
        record.messages.push(message);
    }
    crate::identity::outcomes(fixture, &mut record)?;
    record.reporting.exit_code = Some(reporting::exit_code(&record));
    record.elapsed_ms = started.elapsed().as_millis();
    files::write_json(&run_dir.join("run.json"), &record)?;
    let events = [
        serde_json::json!({"event":"started","at_ms":record.started_ms,"fixture":record.fixture,"profile":record.profile}),
        serde_json::json!({"event":"finished","elapsed_ms":record.elapsed_ms,"status":record.status,"messages":record.messages}),
    ];
    let lines = events
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(io)?
        .join("\n")
        + "\n";
    files::atomic_write(&run_dir.join("events.ndjson"), lines.as_bytes())?;
    Ok(record)
}

fn execute_run(
    root: &Path,
    fixture: &Fixture,
    desktop: bool,
    run_dir: &Path,
    control: &RunControl,
    record: &mut RunRecord,
) -> Result<()> {
    let Some(source) = admit(fixture, control, record) else {
        return Ok(());
    };
    let Some(command) = &fixture.worker else {
        record.status = Status::Blocked;
        prerequisite(
            record,
            "worker",
            Status::Blocked,
            "facade-backed worker is not configured",
        );
        record
            .messages
            .push("facade-backed worker is not configured".into());
        return Ok(());
    };
    let request = request(fixture, record, &source, run_dir, desktop);
    let command = files::expand_command(command, root, &source, run_dir);
    if !worker_available(root, &command, record) {
        return Ok(());
    }
    let client = ProcessRuntimeClient {
        command: &command,
        root,
        run_dir,
        limits: &fixture.limits,
    };
    let report = client.execute_report(&request, control);
    prerequisite(
        record,
        "worker",
        if report.is_ok() {
            Status::Pass
        } else {
            Status::Error
        },
        "worker execution and protocol response",
    );
    apply_report(fixture, run_dir, record, report)
}
fn apply_report(
    fixture: &Fixture,
    run_dir: &Path,
    record: &mut RunRecord,
    report: std::result::Result<WorkerReport, FailureCause>,
) -> Result<()> {
    match report {
        Err(error) => {
            record.status = if error.kind == FailureKind::Cancelled {
                Status::Skipped
            } else {
                Status::Error
            };
            record.messages.push(error.message.clone());
            record.reporting.failure_causes.push(error);
        }
        Ok(worker) => {
            record.worker = Some(worker.clone());
            if worker.evidence_label != fixture.evidence_label {
                return Err("worker evidence_label differs from fixture trust label".into());
            }
            record.captures = evidence::validate(&worker, run_dir, fixture.limits.capture_count)?;
            files::tree_files(run_dir, fixture.limits.artifact_bytes, 4096)?;
            record.assertions = evidence::evaluate(&fixture.assertions, &worker, &record.captures);
            record.status = if record.assertions.iter().any(|a| !a.passed) {
                Status::Fail
            } else if record.assertions.is_empty() {
                Status::Inconclusive
            } else {
                Status::Pass
            };
            if let Err(error) = evidence::validate_teardown(&worker) {
                record.status = Status::Error;
                record.reporting.failure_causes.push(FailureCause::new(
                    FailureKind::Infrastructure,
                    "teardown",
                    error.clone(),
                ));
                record.messages.push(error);
            }
            for diagnostic in &worker.diagnostics {
                record.messages.push(diagnostic.message.clone());
                record.status = reporting::combine(record.status, diagnostic.status);
            }
            if record.status == Status::Fail {
                record.messages.push("one or more assertions failed".into());
            }
            if record.status == Status::Inconclusive {
                record
                    .messages
                    .push("successful worker exit without assertions is inconclusive".into());
            }
        }
    }
    Ok(())
}
fn prerequisite(record: &mut RunRecord, id: &str, status: Status, message: &str) {
    record.reporting.prerequisites.push(Prerequisite {
        id: id.into(),
        status,
        message: message.into(),
        dependencies: if id == "worker" {
            vec!["source".into()]
        } else {
            vec![]
        },
    });
}

fn admit(
    fixture: &Fixture,
    control: &RunControl,
    record: &mut RunRecord,
) -> Option<std::path::PathBuf> {
    if control.is_cancelled() {
        record.status = Status::Skipped;
        record.reporting.failure_causes.push(FailureCause::new(
            FailureKind::Cancelled,
            "admission",
            "cancelled before admission",
        ));
        return None;
    }
    if !fixture.profiles.contains(&record.profile) {
        record.status = Status::Unsupported;
        record.reporting.failure_causes.push(FailureCause::new(
            FailureKind::InvalidSpec,
            "admission",
            "profile is unsupported by fixture",
        ));
        record
            .messages
            .push("profile is unsupported by fixture".into());
        return None;
    }
    let source = match fixture::source_root(fixture) {
        Ok(source) => source,
        Err(error) => {
            record.status = Status::Blocked;
            prerequisite(record, "source", Status::Blocked, &error);
            record.reporting.failure_causes.push(FailureCause::new(
                FailureKind::Infrastructure,
                "source",
                error.clone(),
            ));
            record.messages.push(error);
            return None;
        }
    };
    prerequisite(
        record,
        "source",
        Status::Pass,
        "pinned original game checkout verified",
    );
    Some(source)
}

fn request(
    fixture: &Fixture,
    record: &RunRecord,
    source: &Path,
    run_dir: &Path,
    desktop: bool,
) -> RunRequest {
    RunRequest::for_fixture(
        fixture,
        crate::model::RunTarget {
            run_id: record.run_id.clone(),
            profile: record.profile.clone(),
            desktop,
            source_root: source.to_path_buf(),
            artifact_root: run_dir.to_path_buf(),
        },
    )
}

fn worker_available(
    root: &Path,
    command: &crate::model::CommandSpec,
    record: &mut RunRecord,
) -> bool {
    if !crate::identity::available(root, command) {
        record.status = Status::Blocked;
        let message = "worker unavailable: configured executable is missing";
        prerequisite(record, "worker", Status::Blocked, message);
        record.messages.push(message.into());
        record.reporting.failure_causes.push(FailureCause::new(
            FailureKind::Infrastructure,
            "worker-admission",
            message,
        ));
        return false;
    }
    true
}
