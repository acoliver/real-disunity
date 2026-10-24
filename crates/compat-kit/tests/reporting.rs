use compat_kit::dashboard;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn setup(mode: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    fs::create_dir_all(root.path().join(".work/source")).unwrap();
    let fixture = json!({"schema_version":1,"name":"probe","source_root":".work/source",
        "profiles":["test","unrun"],"default_profile":"test","evidence_label":"harness-only",
        "worker":{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":[mode]},
        "metadata":{"reporting":{"provider_inputs":[],"overlay_inputs":[]}},
        "limits":{"deadline_ms":250,"artifact_bytes":1_048_576},
        "assertions":[{"op":"equal","selector":"counter","pointer":"/count","value":7}]});
    fs::write(root.path().join("fixtures/probe.json"), fixture.to_string()).unwrap();
    root
}
fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_compat-kit"))
        .env("COMPAT_ROOT", root)
        .args(args)
        .output()
        .unwrap()
}
fn edit(root: &Path, change: impl FnOnce(&mut Value)) {
    let path = root.join("fixtures/probe.json");
    let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    change(&mut value);
    fs::write(path, value.to_string()).unwrap();
}
fn saved(root: &Path, id: &str, status: &str) {
    let directory = root.join(".work/compat/runs").join(id);
    fs::create_dir_all(&directory).unwrap();
    let record = json!({"schema_version":1,"run_id":id,"fixture":"probe","profile":"test",
        "evidence_label":"harness-only","status":status,"run_dir":format!(".work/compat/runs/{id}"),
        "started_ms":1,"elapsed_ms":2,"messages":[],"assertions":[],"captures":[],
        "source_before":null,"source_after":null,"worker":null,
        "reporting":{"case_id":"UCK-HAR-001","game":"harness","mode":"simulation",
        "platform":{"os":"linux","arch":"x86_64"},
        "identities":{"source":"source-a","overlay":"overlay-a","provider":"provider-a","test":"test-a"},
        "outcomes":{"compile":"pass","semantic":status,"fidelity":null}}});
    fs::write(directory.join("run.json"), record.to_string()).unwrap();
}

#[test]
fn production_run_uses_completion_exit_dispositions() {
    for (mode, code, kind) in [
        ("pass", 0, None),
        ("mismatch", 1, None),
        ("crash", 3, Some("infrastructure")),
        ("malformed", 3, Some("infrastructure")),
        ("hang", 4, Some("deadline")),
    ] {
        let root = setup(mode);
        let output = cli(root.path(), &["run", "probe"]);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let record: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(record["reporting"]["exit_code"], code);
        if let Some(kind) = kind {
            assert_eq!(record["reporting"]["failure_causes"][0]["kind"], kind);
        }
    }
}

#[test]
fn required_nonpass_and_no_assertions_fail_without_becoming_invalid_invocations() {
    for status in [
        "fail",
        "blocked",
        "unsupported",
        "not-implemented",
        "skipped",
        "flaky",
        "inconclusive",
    ] {
        let root = setup(&format!("diagnostic-{status}"));
        assert_eq!(
            cli(root.path(), &["run", "probe"]).status.code(),
            Some(1),
            "{status}"
        );
    }
    let root = setup("pass");
    edit(root.path(), |f| f["assertions"] = json!([]));
    assert_eq!(cli(root.path(), &["run", "probe"]).status.code(), Some(1));
    assert_eq!(
        cli(root.path(), &["run", "probe", "--profile", "missing"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        cli(root.path(), &["run", "probe", "--unknown"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn all_attempt_reduction_preserves_independent_dimensions_and_conflicts() {
    let root = setup("pass");
    saved(root.path(), "a", "pass");
    saved(root.path(), "b", "fail");
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["runs"].as_array().unwrap().len(), 2);
    let groups = state["groups"].as_array().expect("all-attempt groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["status"], "flaky");
    assert_eq!(groups[0]["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(groups[0]["outcomes"]["compile"], "pass");
    assert_eq!(groups[0]["outcomes"]["semantic"], "flaky");
    assert!(groups[0]["outcomes"]["fidelity"].is_null());
    saved(root.path(), "c", "error");
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["groups"][0]["status"], "error");
    assert_eq!(state["groups"][0]["attempts"].as_array().unwrap().len(), 3);
}

#[test]
fn provider_source_mode_and_unknown_identities_do_not_collapse() {
    let root = setup("pass");
    for (id, field) in [
        ("a", "provider"),
        ("b", "source"),
        ("c", "overlay"),
        ("d", "test"),
    ] {
        saved(root.path(), id, "pass");
        let path = root.path().join(format!(".work/compat/runs/{id}/run.json"));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["reporting"]["identities"][field] = json!(id);
        fs::write(path, record.to_string()).unwrap();
    }
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["groups"].as_array().unwrap().len(), 4);
    for group in state["groups"].as_array().unwrap() {
        assert_eq!(group["status"], "pass");
    }
}

#[test]
fn static_exports_include_all_attempts_filters_and_inert_source_text() {
    let root = setup("pass");
    saved(root.path(), "a", "fail");
    saved(root.path(), "b", "pass");
    let path = root.path().join(".work/compat/runs/a/run.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["messages"] = json!(["</script><img src=x onerror=alert(1)>"]);
    fs::write(path, record.to_string()).unwrap();
    let index = dashboard::generate(root.path(), Path::new(".work/export")).unwrap();
    let html = fs::read_to_string(index).unwrap();
    for id in [
        "case-filter",
        "game-filter",
        "profile-filter",
        "platform-filter",
        "attempt-filter",
    ] {
        assert!(html.contains(id), "missing {id}");
    }
    assert!(html.contains("&lt;/script&gt;&lt;img"));
    assert!(!html.contains("</script><img src=x"));
    let state: Value =
        serde_json::from_slice(&fs::read(root.path().join(".work/export/state.json")).unwrap())
            .unwrap();
    assert_eq!(state["groups"][0]["status"], "flaky");
    assert_eq!(state["runs"].as_array().unwrap().len(), 2);
}

#[test]
fn actual_retries_group_and_one_attempt_with_mixed_assertions_is_never_flaky() {
    let root = setup("alternating");
    for expected in ["pass", "fail", "pass"] {
        let output = cli(root.path(), &["run", "probe"]);
        let record: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(record["status"], expected);
    }
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["groups"].as_array().unwrap().len(), 1);
    assert_eq!(state["groups"][0]["status"], "flaky");
    assert_eq!(state["groups"][0]["attempts"].as_array().unwrap().len(), 3);
    let root = setup("pass");
    edit(root.path(), |f| {
        let mut second = f["assertions"][0].clone();
        second["value"] = json!(99);
        f["assertions"].as_array_mut().unwrap().push(second);
    });
    let output = cli(root.path(), &["run", "probe"]);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["status"], "fail");
    assert_eq!(
        dashboard::state(root.path()).unwrap()["groups"][0]["status"],
        "fail"
    );
}

#[test]
fn explicit_dimensions_use_actual_assertions_and_missing_required_fidelity_is_inconclusive() {
    let root = setup("pass");
    edit(root.path(), |f| {
        f["metadata"]["reporting"] = json!({
        "case_id":"UCK-HAR-001","game":"harness","assertion_dimensions":{"0":"semantic"},
        "units":{"0":"count"},"required_dimensions":["semantic","fidelity"]});
    });
    let output = cli(root.path(), &["run", "probe"]);
    assert_eq!(output.status.code(), Some(1));
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["status"], "inconclusive");
    assert_eq!(record["reporting"]["outcomes"]["semantic"], "pass");
    assert!(record["reporting"]["outcomes"]["fidelity"].is_null());
    assert_eq!(record["assertions"][0]["unit"], "count");
    assert_eq!(
        record["assertions"][0]["actual"]["latest_first"][0]["value"],
        7
    );
}

#[test]
fn unknown_legacy_records_remain_separate_and_exit_precedence_is_structured() {
    let root = setup("pass");
    saved(root.path(), "a", "pass");
    saved(root.path(), "b", "fail");
    for id in ["a", "b"] {
        let path = root.path().join(format!(".work/compat/runs/{id}/run.json"));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record.as_object_mut().unwrap().remove("reporting");
        fs::write(path, record.to_string()).unwrap();
    }
    assert_eq!(
        dashboard::state(root.path()).unwrap()["groups"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (mut records, _) = dashboard::records(root.path()).unwrap();
    let record = &mut records[0];
    record.status = compat_kit::Status::Fail;
    for (kind, code) in [
        ("invalid-spec", 2),
        ("infrastructure", 3),
        ("deadline", 4),
        ("cancelled", 4),
    ] {
        let cause = serde_json::from_value(
            json!({"kind":kind,"stage":"test","message":"literal, not parsed"}),
        )
        .unwrap();
        record.reporting.failure_causes.push(cause);
        assert_eq!(compat_kit::reporting::exit_code(record), code);
    }
}

#[test]
fn changed_non_script_inputs_and_provider_images_split_real_attempts() {
    let root = setup("pass");
    let provider = root.path().join(".work/provider.bin");
    fs::write(&provider, "provider-a").unwrap();
    edit(root.path(), |f| {
        f["metadata"]["reporting"] =
            json!({"provider_inputs":[".work/provider.bin"],"overlay_inputs":[]});
    });
    cli(root.path(), &["run", "probe"]);
    fs::write(root.path().join(".work/source/scene.unity"), "scene-b").unwrap();
    cli(root.path(), &["run", "probe"]);
    fs::write(&provider, "provider-b").unwrap();
    cli(root.path(), &["run", "probe"]);
    assert_eq!(
        dashboard::state(root.path()).unwrap()["groups"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn invalid_reporting_selection_is_rejected_as_spec_and_missing_runtime_is_infrastructure() {
    let root = setup("pass");
    edit(root.path(), |f| {
        f["metadata"]["reporting"] = json!({"assertion_dimensions":{"99":"semantic"}});
    });
    assert_eq!(cli(root.path(), &["run", "probe"]).status.code(), Some(2));
    edit(root.path(), |f| {
        f["metadata"] = json!({});
        f["worker"]["program"] = json!("/missing/provider");
    });
    let output = cli(root.path(), &["run", "probe"]);
    assert_eq!(output.status.code(), Some(3));
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["status"], "blocked");
    assert_eq!(
        record["reporting"]["failure_causes"][0]["kind"],
        "infrastructure"
    );
}

#[test]
fn junit_retains_every_attempt_and_maps_nine_original_statuses() {
    let root = setup("pass");
    let statuses = [
        "pass",
        "fail",
        "unsupported",
        "not-implemented",
        "blocked",
        "skipped",
        "error",
        "flaky",
        "inconclusive",
    ];
    for (index, status) in statuses.iter().enumerate() {
        saved(root.path(), &format!("attempt-{index}"), status);
    }
    dashboard::generate(root.path(), Path::new(".work/export")).unwrap();
    let xml = fs::read_to_string(root.path().join(".work/export/junit.xml")).expect("JUnit export");
    assert_eq!(xml.matches("<testcase ").count(), 9);
    assert_eq!(xml.matches("<failure ").count(), 2);
    assert_eq!(xml.matches("<error ").count(), 1);
    assert_eq!(xml.matches("<skipped ").count(), 5);
    for status in statuses {
        assert!(xml.contains(&format!("name=\"status\" value=\"{status}\"")));
    }
}

#[test]
fn a_worker_pending_diagnostic_cannot_hide_a_measured_assertion_failure() {
    let root = setup("diagnostic-inconclusive");
    edit(root.path(), |f| f["assertions"][0]["value"] = json!(99));
    let output = cli(root.path(), &["run", "probe"]);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["status"], "fail");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn measured_dimension_diagnostics_preserve_blockers_without_promoting_untested_fidelity() {
    let root = setup("diagnostic-blocked");
    edit(root.path(), |f| {
        f["metadata"]["reporting"] = json!({
        "assertion_dimensions":{"0":"semantic"},"diagnostic_dimensions":{"0":"content"}});
    });
    let output = cli(root.path(), &["run", "probe"]);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["reporting"]["outcomes"]["semantic"], "pass");
    assert_eq!(record["reporting"]["outcomes"]["content"], "blocked");
    assert!(record["reporting"]["outcomes"]["fidelity"].is_null());
    edit(root.path(), |f| {
        f["worker"]["args"] = json!(["diagnostic-pass"]);
        f["metadata"]["reporting"]["diagnostic_dimensions"] = json!({"0":"fidelity"});
    });
    let output = cli(root.path(), &["run", "probe"]);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(record["reporting"]["outcomes"]["fidelity"].is_null());
}

#[test]
fn undeclared_qualification_inputs_keep_real_attempts_separate() {
    let root = setup("alternating");
    edit(root.path(), |f| f["metadata"] = json!({}));
    cli(root.path(), &["run", "probe"]);
    cli(root.path(), &["run", "probe"]);
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["groups"].as_array().unwrap().len(), 2);
    for run in state["runs"].as_array().unwrap() {
        assert!(run["reporting"]["identities"]["provider"].is_null());
        assert!(run["reporting"]["identities"]["overlay"].is_null());
    }
}

#[test]
fn case_game_mode_platform_profile_fixture_and_trust_boundaries_split_groups() {
    let root = setup("pass");
    let changes = [
        ("case", "/reporting/case_id", json!("another-case")),
        ("game", "/reporting/game", json!("another-game")),
        ("mode", "/reporting/mode", json!("desktop-replay")),
        ("platform", "/reporting/platform/os", json!("windows")),
        ("profile", "/profile", json!("another-profile")),
        ("fixture", "/fixture", json!("another-fixture")),
        ("trust", "/evidence_label", json!("original")),
    ];
    saved(root.path(), "baseline", "pass");
    for (id, pointer, value) in changes {
        saved(root.path(), id, "fail");
        let path = root.path().join(format!(".work/compat/runs/{id}/run.json"));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        *record.pointer_mut(pointer).unwrap() = value;
        fs::write(path, record.to_string()).unwrap();
    }
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["runs"].as_array().unwrap().len(), 8);
    let groups = state["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 8);
    assert!(
        groups
            .iter()
            .all(|g| g["attempts"].as_array().unwrap().len() == 1)
    );
    assert!(groups.iter().all(|g| g["status"] != "flaky"));
}

#[test]
fn pending_repeats_preserve_their_status_and_all_attempt_evidence() {
    for status in [
        "error",
        "blocked",
        "unsupported",
        "not-implemented",
        "skipped",
        "inconclusive",
    ] {
        let root = setup("pass");
        saved(root.path(), "passing", "pass");
        saved(root.path(), "failing", "fail");
        saved(root.path(), "pending", status);
        let state = dashboard::state(root.path()).unwrap();
        assert_eq!(state["groups"][0]["status"], status);
        assert_eq!(state["groups"][0]["attempts"].as_array().unwrap().len(), 3);
        assert_eq!(state["runs"].as_array().unwrap().len(), 3);
        assert_ne!(state["groups"][0]["exit_code"], 0);
    }
}

#[test]
fn legacy_assertion_source_and_status_fields_roundtrip_without_invented_evidence() {
    let root = setup("pass");
    saved(root.path(), "legacy", "not_implemented");
    let path = root.path().join(".work/compat/runs/legacy/run.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record.as_object_mut().unwrap().remove("reporting");
    record["assertions"] = json!([{"assertion":{"op":"equal","selector":"counter","value":7},
        "passed":false,"message":"legacy assertion evidence"}]);
    record["source_before"] =
        json!({"root":"legacy-source","revision":null,"dirty":null,"scripts":{}});
    fs::write(path, record.to_string()).unwrap();
    dashboard::generate(root.path(), Path::new(".work/export")).unwrap();
    let state: Value =
        serde_json::from_slice(&fs::read(root.path().join(".work/export/state.json")).unwrap())
            .unwrap();
    let run = &state["runs"][0];
    assert_eq!(run["status"], "not-implemented");
    assert_eq!(run["assertions"][0]["passed"], false);
    for field in ["expected", "actual", "unit"] {
        assert!(run["assertions"][0][field].is_null(), "{field}");
    }
    assert_eq!(run["source_before"]["inputs"], json!({}));
    assert!(
        state["groups"][0]["outcomes"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_null)
    );
    assert_eq!(state["groups"][0]["identity"]["unknown_attempt"], "legacy");
}

#[test]
fn exports_retain_every_attempt_beyond_the_former_250_record_limit() {
    let root = setup("pass");
    for index in 0..257 {
        saved(
            root.path(),
            &format!("attempt-{index}"),
            if index == 256 { "fail" } else { "pass" },
        );
    }
    dashboard::generate(root.path(), Path::new(".work/export")).unwrap();
    let state: Value =
        serde_json::from_slice(&fs::read(root.path().join(".work/export/state.json")).unwrap())
            .unwrap();
    assert_eq!(state["runs"].as_array().unwrap().len(), 257);
    assert_eq!(
        state["groups"][0]["attempts"].as_array().unwrap().len(),
        257
    );
    assert_eq!(state["groups"][0]["status"], "flaky");
    assert!(
        state["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["run_id"] == "attempt-256" && r["status"] == "fail")
    );
    let xml = fs::read_to_string(root.path().join(".work/export/junit.xml")).unwrap();
    assert_eq!(xml.matches("<testcase ").count(), 257);
    assert_eq!(xml.matches("<failure ").count(), 1);
}

#[test]
fn report_safety_bounds_reject_incomplete_aggregates() {
    let root = setup("pass");
    let runs = root.path().join(".work/compat/runs");
    fs::create_dir_all(&runs).unwrap();
    for index in 0..10_001 {
        fs::create_dir(runs.join(index.to_string())).unwrap();
    }
    assert!(
        dashboard::state(root.path())
            .unwrap_err()
            .contains("entry cap")
    );
    let root = setup("pass");
    saved(root.path(), "oversized", "pass");
    fs::OpenOptions::new()
        .write(true)
        .open(root.path().join(".work/compat/runs/oversized/run.json"))
        .unwrap()
        .set_len(67_108_865)
        .unwrap();
    assert!(
        dashboard::state(root.path())
            .unwrap_err()
            .contains("byte cap")
    );
}

#[test]
fn actual_desktop_requests_split_modes_and_do_not_promote_desktop_or_fidelity() {
    let root = setup("pass");
    for args in [vec!["run", "probe"], vec!["run", "probe", "--desktop"]] {
        let output = cli(root.path(), &args);
        assert_eq!(output.status.code(), Some(0));
        let record: Value = serde_json::from_slice(&output.stdout).unwrap();
        for dimension in [
            "compile",
            "content",
            "semantic",
            "desktop",
            "exploratory",
            "fidelity",
        ] {
            assert!(
                record["reporting"]["outcomes"][dimension].is_null(),
                "{dimension}"
            );
        }
        let request: Value = serde_json::from_slice(
            &fs::read(
                root.path()
                    .join(record["run_dir"].as_str().unwrap())
                    .join("request.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(request["desktop"], args.contains(&"--desktop"));
    }
    assert_eq!(
        dashboard::state(root.path()).unwrap()["groups"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn executed_run_identifies_the_tck_run_wire_contract() {
    let root = setup("pass");
    let output = cli(root.path(), &["run", "probe"]);
    assert!(output.status.success());
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["schema"], "real-disunity.tck-run/1");
    let path = root
        .path()
        .join(record["run_dir"].as_str().unwrap())
        .join("run.json");
    let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(persisted, record);
}

#[test]
fn legacy_run_roundtrip_retains_missing_wire_identity_and_unknown_measurements() {
    let root = setup("pass");
    saved(root.path(), "legacy-schema", "inconclusive");
    let path = root.path().join(".work/compat/runs/legacy-schema/run.json");
    let record: compat_kit::model::RunRecord =
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let value = serde_json::to_value(record).unwrap();
    assert!(value.get("schema").is_none());
    assert_eq!(value["status"], "inconclusive");
    assert!(value["reporting"]["outcomes"]["fidelity"].is_null());
}

#[test]
fn explicit_unknown_tck_run_contract_is_not_parsed_as_a_legacy_record() {
    let root = setup("pass");
    saved(root.path(), "wrong-schema", "pass");
    let path = root.path().join(".work/compat/runs/wrong-schema/run.json");
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    for schema in [
        json!("real-disunity.tck-run/2"),
        json!("other/1"),
        json!(4),
        Value::Null,
    ] {
        value["schema"] = schema;
        assert!(serde_json::from_value::<compat_kit::model::RunRecord>(value.clone()).is_err());
    }
}
