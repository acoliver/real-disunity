use super::*;
use std::path::PathBuf;

fn setup() -> (tempfile::TempDir, PathBuf, RunRequest) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let request = RunRequest {
        schema_version: 1,
        run_id: "resources".into(),
        fixture: "resources".into(),
        profile: "compat-6000.3".into(),
        desktop: false,
        seed: 1,
        timeline: vec![],
        assertions: vec![],
        source_root: directory.path().join("source"),
        artifact_root: root.join(".work/artifacts"),
        metadata: json!({}),
    };
    fs::create_dir_all(request.source_root.join("ProjectSettings")).unwrap();
    (directory, root, request)
}

#[test]
fn font_reads_reject_absolute_and_relative_escape() {
    let (directory, root, mut request) = setup();
    let outside = directory.path().join("external.ttf");
    fs::write(&outside, b"font").unwrap();
    for path in [outside, PathBuf::from("../external.ttf")] {
        request.metadata = json!({"fonts":{"default":path}});
        assert!(
            commands(&root, &request).is_err(),
            "escaping font read must be rejected"
        );
    }
}

#[test]
fn contained_absolute_fonts_and_authorized_external_input_settings_are_readable() {
    let (_directory, root, mut request) = setup();
    let font = root.join("font.ttf");
    fs::write(&font, b"font").unwrap();
    request.metadata = json!({"fonts":{"default":font}});
    fs::write(
        request
            .source_root
            .join("ProjectSettings/InputManager.asset"),
        "%YAML 1.1\n--- !u!13 &1\nInputManager:\n  m_Axes: []\n",
    )
    .unwrap();
    let values = commands(&root, &request).unwrap();
    assert_eq!(values[0]["data_base64"], STANDARD.encode(b"font"));
    assert_eq!(values[1], json!({"op":"input.configure_axes","axes":{}}));
}

#[test]
fn oversized_and_empty_fonts_and_oversized_input_are_rejected() {
    let (_directory, root, mut request) = setup();
    let font = root.join("font.ttf");
    request.metadata = json!({"fonts":{"default":font}});
    for size in [0, 780_001] {
        fs::File::create(&font).unwrap().set_len(size).unwrap();
        assert!(commands(&root, &request).is_err());
    }
    request.metadata = json!({});
    fs::File::create(
        request
            .source_root
            .join("ProjectSettings/InputManager.asset"),
    )
    .unwrap()
    .set_len(1_048_577)
    .unwrap();
    assert!(commands(&root, &request).unwrap_err().contains("budget"));
}

#[cfg(unix)]
#[test]
fn resource_symlinks_cannot_read_outside_authorized_roots() {
    use std::os::unix::fs::symlink;
    let (directory, root, mut request) = setup();
    let outside = directory.path().join("external.ttf");
    fs::write(&outside, b"font").unwrap();
    symlink(&outside, root.join("font.ttf")).unwrap();
    request.metadata = json!({"fonts":{"default":"font.ttf"}});
    assert!(commands(&root, &request).is_err());
    request.metadata = json!({});
    symlink(
        outside,
        request
            .source_root
            .join("ProjectSettings/InputManager.asset"),
    )
    .unwrap();
    assert!(commands(&root, &request).unwrap_err().contains("symlink"));
}
