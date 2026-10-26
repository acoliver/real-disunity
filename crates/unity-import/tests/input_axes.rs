//! Malformed input manager entries fail with their position; a missing file has no axes.
#[path = "support/fixture.rs"]
mod fixture;

use fixture::Fixture;
use worldrunner_unity_import::input_axes;

const AXIS: &str = "  - m_Name: Jump\n    negativeButton: \n    positiveButton: space\n    gravity: 1000\n    dead: 0.001\n    sensitivity: 1000\n    snap: 0\n    invert: 0\n    type: 0\n";

/// A project with an empty `Assets` folder.
fn project(editor: &str) -> Fixture {
    let fixture = Fixture::new(editor);
    std::fs::create_dir(fixture.0.path().join("Assets")).unwrap();
    fixture
}

fn manager(axes: &str) -> Fixture {
    let fixture = project("2017.1.0f3");
    fixture.file(
        "ProjectSettings/InputManager.asset",
        format!("%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n--- !u!13 &1\nInputManager:\n  m_Axes:\n{axes}"),
    );
    fixture
}

#[test]
fn missing_input_manager_means_no_axes() {
    assert!(
        input_axes(&project("6000.3.5f2").project())
            .unwrap()
            .is_empty()
    );
    let axes = input_axes(&manager(AXIS).project()).unwrap();
    assert_eq!(axes.len(), 1);
    assert_eq!(
        (axes[0].name.as_str(), axes[0].positive.as_str()),
        ("Jump", "space")
    );
    assert!(axes[0].negative.is_empty() && !axes[0].snap);
}

#[test]
fn malformed_axes_fail_with_their_position() {
    for (axes, expected) in [
        (AXIS.replace("type: 0", "type: 5"), "invalid type"),
        (
            AXIS.replace("gravity: 1000", "gravity: fast"),
            "invalid gravity",
        ),
        (AXIS.replace("snap: 0", "snap: 2"), "invalid snap"),
        (AXIS.replace("m_Name: Jump", "m_Name: "), "has no m_Name"),
        (AXIS.replace("    dead: 0.001\n", ""), "invalid dead"),
    ] {
        let message = input_axes(&manager(&format!("{AXIS}{axes}")).project())
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("InputManager.asset")
                && message.contains(&format!("axis 1: {expected}")),
            "{message}"
        );
    }
}
