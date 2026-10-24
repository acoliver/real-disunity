use super::{Command, Input};
use serde_json::json;
use worldrunner::scene::viewer::LiveInputEvent;

fn key(key: &str, pressed: bool) -> LiveInputEvent {
    LiveInputEvent::Key {
        key: key.into(),
        pressed,
        repeat: false,
    }
}

#[test]
fn os_and_script_keys_are_unioned_and_focus_loss_releases_only_os_keys() {
    let mut input = Input::default();
    input
        .script(&json!({"keys":["A"],"axes":{"Horizontal":0.5}}))
        .unwrap();
    assert!(
        input
            .ingress(&[key("a", true), key("d", true)], false)
            .changed
    );
    assert_eq!(
        input.state(),
        (vec!["A".into(), "D".into()], json!({"Horizontal":0.5}))
    );
    assert!(
        input
            .ingress(&[LiveInputEvent::Focused(false)], false)
            .changed
    );
    assert_eq!(input.state().0, vec!["A"]);
    assert!(!input.ingress(&[key("d", false)], false).changed);
}

#[test]
fn reload_clears_script_axes_and_held_os_keys() {
    let mut input = Input::default();
    input
        .script(&json!({"keys":["A"],"axes":{"Horizontal":1.0}}))
        .unwrap();
    input.ingress(&[key("d", true)], true);
    input.clear();
    assert_eq!(input.state(), (vec![], json!({})));
    assert!(input.ingress(&[key("d", true)], true).changed);
    assert_eq!(input.state().0, vec!["D"]);
}

#[test]
fn controls_are_edge_triggered_and_do_not_enter_game_input() {
    let mut input = Input::default();
    let first = input.ingress(
        &[key("f5", true), key("f12", true), key("escape", true)],
        true,
    );
    assert_eq!(
        first.commands,
        vec![Command::Reload, Command::Capture, Command::Stop]
    );
    assert!(!first.changed);
    assert!(input.state().0.is_empty());
    assert!(input.ingress(&[key("f5", true)], true).commands.is_empty());
    input.ingress(&[key("f5", false)], true);
    assert_eq!(
        input.ingress(&[key("f5", true)], true).commands,
        vec![Command::Reload]
    );
    let repeat = LiveInputEvent::Key {
        key: "f12".into(),
        pressed: true,
        repeat: true,
    };
    assert!(input.ingress(&[repeat], true).commands.is_empty());
}

#[test]
fn replay_does_not_reserve_interactive_hotkeys() {
    let mut input = Input::default();
    let result = input.ingress(&[key("f5", true), key("escape", true)], false);
    assert!(result.commands.is_empty());
    assert_eq!(input.state().0.len(), 2);
}

#[test]
fn releases_and_focus_loss_allow_a_new_control_edge() {
    let mut input = Input::default();
    input.ingress(&[key("f12", true)], true);
    input.ingress(&[LiveInputEvent::Focused(false)], true);
    assert_eq!(
        input.ingress(&[key("f12", true)], true).commands,
        vec![Command::Capture]
    );
    assert_eq!(
        input
            .ingress(&[LiveInputEvent::CloseRequested], true)
            .commands,
        vec![Command::Stop]
    );
}
