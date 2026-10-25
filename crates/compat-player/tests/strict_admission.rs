use compat_kit::RunRequest;
use compat_player::{Installation, run_request};
use serde_json::{Value, json};

fn request(metadata: Value, action: Value) -> RunRequest {
    RunRequest {
        schema_version: 1,
        run_id: "strict-admission".into(),
        fixture: "generic".into(),
        profile: "compat-6000.3".into(),
        desktop: false,
        seed: 1,
        source_root: "/unavailable-originals".into(),
        artifact_root: "/unavailable-artifacts".into(),
        assertions: vec![],
        metadata,
        timeline: vec![json!({"frame":0,"action":"load"}), action],
    }
}

fn rejects(request: &RunRequest, reason: &str) {
    let report = run_request(&unavailable(), request);
    assert_eq!(report.diagnostics.len(), 1);
    assert!(
        report.diagnostics[0].message.contains(reason),
        "{:?}",
        report.diagnostics
    );
    assert!(!report.teardown.completed);
}

#[test]
fn explicit_unknown_profiles_reject_before_compilation_or_activation() {
    for profile in [json!(null), json!(false), json!("native_scheduler_v1")] {
        rejects(
            &request(
                json!({"execution_profile":profile}),
                json!({"action":"observe"}),
            ),
            "execution_profile",
        );
    }
}

#[test]
fn strict_and_legacy_actions_cannot_cross_modes_before_activation() {
    for action in ["input", "advance"] {
        rejects(
            &request(
                json!({"execution_profile":"native_phase_context_v1"}),
                json!({"action":action}),
            ),
            "execution_profile",
        );
    }
    for action in ["input_events", "advance_rational"] {
        rejects(
            &request(json!({}), json!({"action":action})),
            "execution_profile",
        );
    }
}

#[test]
fn strict_cached_assemblies_and_malformed_ratios_reject_before_compilation() {
    for field in ["assembly", "assembly_qualification"] {
        let mut metadata = json!({"execution_profile":"native_phase_context_v1"});
        metadata[field] = json!("/missing.dll");
        rejects(
            &request(metadata, json!({"action":"observe"})),
            "fresh original-source",
        );
    }
    rejects(
        &request(
            json!({"execution_profile":"native_phase_context_v1"}),
            json!({"action":"advance_rational","dt":{"num":1,"den":0}}),
        ),
        "rational",
    );
}

#[test]
fn strict_malformed_tail_rejects_before_the_load_mutates_a_world() {
    rejects(
        &request(
            json!({"execution_profile":"native_phase_context_v1"}),
            json!({
            "action":"input_events","events":[
                {"event_id":1,"timestamp":{"num":0,"den":1},"source":"replay","kind":"key","key":"W","pressed":true},
                {"event_id":2,"timestamp":{"num":0,"den":1},"source":"replay","kind":"axis","axis":"Horizontal","value":2}
            ]}),
        ),
        "normalized axis",
    );
}

/// Admission must reject before touching the workspace or loading the native runtime.
fn unavailable() -> Installation {
    Installation::new("/unavailable-root", "/unavailable-native-library")
}
