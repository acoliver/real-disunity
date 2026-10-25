use compat_kit::RunRequest;
use compat_player::{Installation, run_request};
use serde_json::{Value, json};

fn request(interactive: &Value) -> RunRequest {
    RunRequest {
        schema_version: 1,
        run_id: "interactive-admission".into(),
        fixture: "generic".into(),
        profile: "compat-6000.3".into(),
        desktop: false,
        seed: 1,
        source_root: "/unavailable-originals".into(),
        artifact_root: "/unavailable-artifacts".into(),
        assertions: vec![],
        metadata: json!({"interactive": interactive}),
        timeline: vec![json!({"frame":0,"phase":"load","action":"load"})],
    }
}

#[test]
fn interactive_requires_desktop_before_source_activation() {
    let mut request = request(&json!({"duration_ms":30000,"max_frames":1800,"max_captures":8}));
    request.timeline.clear();
    let report = run_request(&unavailable(), &request);
    assert_eq!(report.diagnostics.len(), 1);
    assert!(
        report.diagnostics[0]
            .message
            .contains("interactive requires desktop"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.captures.is_empty());
    assert!(!report.teardown.completed);
}

#[test]
fn malformed_interactive_metadata_is_rejected_before_activation() {
    for config in [
        Value::Null,
        json!(true),
        json!({}),
        json!({"duration_ms":30000,"max_frames":1800,"max_captures":8,"unknown":1}),
        json!({"duration_ms":"30000","max_frames":1800,"max_captures":8}),
        json!({"duration_ms":0,"max_frames":1800,"max_captures":8}),
        json!({"duration_ms":30000,"max_frames":0,"max_captures":8}),
        json!({"duration_ms":30000,"max_frames":1800,"max_captures":65}),
        json!({"duration_ms":60001,"max_frames":1800,"max_captures":8}),
        json!({"duration_ms":30000,"max_frames":36001,"max_captures":8}),
        json!({"duration_ms":30000,"max_frames":1800,"max_captures":0}),
        json!({"duration_ms":-1,"max_frames":1800,"max_captures":8}),
        json!({"duration_ms":30000.0,"max_frames":1800,"max_captures":8}),
    ] {
        let mut request = request(&config);
        request.desktop = true;
        let report = run_request(&unavailable(), &request);
        assert_eq!(report.diagnostics.len(), 1);
        assert!(
            report.diagnostics[0].message.contains("interactive"),
            "{:?}",
            report.diagnostics
        );
        assert!(!report.teardown.completed);
    }
}

#[test]
fn interactive_rejects_replay_actions_and_cached_assemblies() {
    for conflict in ["advance", "reload", "unload", "input", "assembly"] {
        let mut request = request(&json!({"duration_ms":30000,"max_frames":1800,"max_captures":8}));
        request.desktop = true;
        if conflict == "assembly" {
            request.metadata["assembly"] = json!("/unavailable.dll");
        } else {
            request
                .timeline
                .push(json!({"frame":1,"phase":"update","action":conflict,"dt":0.1}));
        }
        let report = run_request(&unavailable(), &request);
        assert!(
            report.diagnostics[0].message.contains("interactive"),
            "{:?}",
            report.diagnostics
        );
        assert!(!report.teardown.completed);
    }
}

/// Admission must reject before touching the workspace or loading the native runtime.
fn unavailable() -> Installation {
    Installation::new("/unavailable-root", "/unavailable-native-library")
}
