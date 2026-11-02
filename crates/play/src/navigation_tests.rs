use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{MouseButton, RetainedInput, WheelDelta};

use crate::navigation::Navigation;
use crate::scene_transform::orbit_limits;

const SIZE: [u32; 2] = [960, 540];

fn pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [0, -10_000, 5_000],
        target_millimeters: [0, 0, 0],
    }
}

fn started() -> Navigation {
    let mut navigation = Navigation::default();
    navigation.start(pose(), orbit_limits()).unwrap();
    navigation
}

fn press(navigation: &mut Navigation, pressed: bool) {
    navigation.accept(
        &RetainedInput::Button {
            button: MouseButton::Left,
            pressed,
        },
        SIZE,
    );
}

fn key(name: &str) -> RetainedInput {
    RetainedInput::Key {
        key: name.to_owned(),
        logical: worldrunner_scene_viewer::LogicalKey::Dead(None),
        physical: worldrunner_scene_viewer::PhysicalKey::Unidentified(
            worldrunner_scene_viewer::NativeKeyCode::Unidentified,
        ),
        pressed: true,
        repeat: false,
    }
}

#[test]
fn moving_the_pointer_without_a_button_moves_nothing() {
    let mut navigation = started();
    navigation.accept(&RetainedInput::PointerMoved([10.0, 10.0]), SIZE);
    navigation.accept(&RetainedInput::PointerMoved([200.0, 90.0]), SIZE);
    assert_eq!(navigation.take_moved(), None);
}

#[test]
fn dragging_with_the_left_button_orbits_and_the_pose_is_taken_once() {
    let mut navigation = started();
    navigation.accept(&RetainedInput::PointerMoved([10.0, 10.0]), SIZE);
    press(&mut navigation, true);
    navigation.accept(&RetainedInput::PointerMoved([200.0, 10.0]), SIZE);
    let moved = navigation.take_moved().expect("the drag moved the camera");
    assert_ne!(moved.eye_millimeters, pose().eye_millimeters);
    assert_eq!(moved.target_millimeters, pose().target_millimeters);
    assert_eq!(navigation.take_moved(), None);
}

#[test]
fn releasing_the_button_ends_the_drag() {
    let mut navigation = started();
    navigation.accept(&RetainedInput::PointerMoved([10.0, 10.0]), SIZE);
    press(&mut navigation, true);
    press(&mut navigation, false);
    navigation.accept(&RetainedInput::PointerMoved([200.0, 10.0]), SIZE);
    assert_eq!(navigation.take_moved(), None);
}

#[test]
fn the_wheel_zooms_towards_the_target_in_lines_or_pixels() {
    for delta in [
        WheelDelta::Lines([0.0, 2.0]),
        WheelDelta::Pixels([0.0, 80.0]),
    ] {
        let mut navigation = started();
        navigation.accept(&RetainedInput::Wheel(delta), SIZE);
        let moved = navigation.take_moved().expect("the wheel moved the camera");
        assert!(moved.eye_millimeters[1] > -10_000, "{moved:?}");
    }
}

#[test]
fn r_and_home_return_to_the_starting_pose() {
    for name in ["r", "home"] {
        let mut navigation = started();
        navigation.accept(&RetainedInput::Wheel(WheelDelta::Lines([0.0, 3.0])), SIZE);
        navigation.take_moved();
        navigation.accept(&key(name), SIZE);
        assert_eq!(navigation.take_moved(), Some(pose()), "{name}");
    }
}

#[test]
fn other_keys_do_not_move_the_camera() {
    let mut navigation = started();
    navigation.accept(&key("w"), SIZE);
    assert_eq!(navigation.take_moved(), None);
}

#[test]
fn before_a_start_the_input_moves_nothing() {
    let mut navigation = Navigation::default();
    press(&mut navigation, true);
    navigation.accept(&RetainedInput::PointerMoved([1.0, 1.0]), SIZE);
    navigation.accept(&RetainedInput::PointerMoved([90.0, 40.0]), SIZE);
    navigation.accept(&RetainedInput::Wheel(WheelDelta::Lines([0.0, 1.0])), SIZE);
    assert_eq!(navigation.take_moved(), None);
}

#[test]
fn forty_wheel_pixels_zoom_as_far_as_one_line() {
    let mut by_lines = started();
    by_lines.accept(&RetainedInput::Wheel(WheelDelta::Lines([0.0, 1.0])), SIZE);
    let mut by_pixels = started();
    by_pixels.accept(&RetainedInput::Wheel(WheelDelta::Pixels([0.0, 40.0])), SIZE);
    assert_eq!(by_pixels.take_moved(), by_lines.take_moved());
}
