use std::process::Command;
use worldrunner_render_scene::offscreen::RgbaImage;

pub(super) const TITLE: &str = "worldrunner-live-contract-test";

pub(super) fn landmarks(image: &RgbaImage, frame: u64) {
    let red_x: Vec<_> = image
        .pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, p)| p[0] > 250 && p[1] < 2 && p[2] < 2)
        .map(|(i, _)| u32::try_from(i).unwrap() % image.viewport.width)
        .collect();
    let side = image.viewport.height / 4;
    assert_eq!(
        red_x.len(),
        usize::try_from(side * side).unwrap(),
        "quad has projected GPU area"
    );
    let step = image.viewport.height / 8;
    let expected = image.viewport.width / 2 - step + step * u32::try_from(frame.min(2)).unwrap();
    assert_eq!(*red_x.iter().min().unwrap(), expected);
}

fn window() -> String {
    let search = Command::new("xdotool")
        .args(["search", "--name", TITLE])
        .output()
        .unwrap();
    assert!(search.status.success());
    String::from_utf8(search.stdout)
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .to_owned()
}

pub(super) fn key(action: &str) {
    let id = window();
    assert!(
        Command::new("xdotool")
            .args(["windowfocus", "--sync", &id])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("xdotool")
            .args([action, "Right"])
            .status()
            .unwrap()
            .success()
    );
}

pub(super) fn resize() {
    assert!(
        Command::new("xdotool")
            .args(["windowsize", &window(), "192", "96"])
            .status()
            .unwrap()
            .success()
    );
}
