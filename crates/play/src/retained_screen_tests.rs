//! The windowed player shows the game's own frame: the image of the screen the player's GPU work
//! drew into covers the stock scene, and the game's UI draws on top of it.
//!
//! One test plays the portable screen fixture, whose script clears the screen to yellow with a
//! command buffer, so the window shows that frame with the offscreen work on a device. The other
//! puts an image with a known top and bottom into the sprite textures the way the driver does
//! for the image it reads back, and checks which way up the window shows it and that the UI of the
//! retained UI fixture stays over it. Both need a GPU adapter (lavapipe in CI), and the colours
//! use the channel values 0 and 1, which stay the same through every colour-space encoding.

use super::ui_tests::{booted_fixture_host, fixture_config, fixture_turn};
use super::*;
use std::{path::PathBuf, time::Instant};

use crate::PreparedPlayer;
use crate::offscreen::ScreenImage;
use crate::player::player_gpu;
use crate::player_textures::SCREEN_IMAGE;
use crate::scene_light::SceneLighting;

const SIZE: u32 = 64;
const YELLOW: [u8; 3] = [255, 255, 0];
const MAGENTA: [u8; 3] = [255, 0, 255];
const CYAN: [u8; 3] = [0, 255, 255];

fn window(gpu_work: GpuWork, capture_frames: &[u64]) -> PlayerWindow {
    PlayerWindow {
        title: "windowless screen".into(),
        size: [SIZE, SIZE],
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames: None,
        capture_frames: capture_frames.iter().copied().collect(),
        keys: vec![],
        gpu_work,
    }
}

/// Runs `frames` updates of `driver` in the windowless viewer and waits for its captures.
fn run_frames(viewer: RetainedHost, driver: &mut PlayerRetainedDriver, frames: u64) {
    let mut run = viewer.windowless(driver).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    for _ in 0..frames {
        run.frame().unwrap();
        run.poll().unwrap();
    }
    while run.poll().unwrap() {
        assert!(Instant::now() < deadline, "capture timed out");
        std::thread::yield_now();
    }
    run.shutdown().unwrap();
}

fn capture_of(driver: &PlayerRetainedDriver, frame: u64) -> &Capture {
    driver
        .captures
        .iter()
        .find(|capture| capture.frame == frame)
        .unwrap_or_else(|| panic!("no capture of frame {frame}"))
}

/// Plays the screen fixture in a window for four updates, capturing the last, with the GPU work
/// the window asks for. Returns the driver after the run.
fn play_screen_fixture(gpu_work: GpuWork) -> PlayerRetainedDriver {
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture/screen.fixture"),
        data: data.path().to_owned(),
    };
    let gpu = player_gpu(gpu_work, &prepared.data);
    assert_eq!(
        gpu.is_some(),
        gpu_work == GpuWork::Auto,
        "the test needs a GPU adapter"
    );
    let (viewer, mut driver) = start_window(&prepared, window(gpu_work, &[3]), gpu).unwrap();
    run_frames(viewer, &mut driver, 4);
    driver
}

#[test]
fn the_window_shows_the_screen_a_script_cleared_over_the_stock_scene() {
    let _turn = fixture_turn();
    let driver = play_screen_fixture(GpuWork::Auto);

    let capture = capture_of(&driver, 3);
    for (x, y) in [(0, 0), (31, 31), (63, 63), (5, 58), (60, 3)] {
        assert_eq!(capture.rgb(x, y), YELLOW, "the window at {x},{y}");
    }
    assert!(
        driver.sprites.textures.contains_key(&SCREEN_IMAGE),
        "the image of the screen is among the sprite textures"
    );
    assert_eq!(driver.screen_shown, Some(0), "the frame whose work drew it");
    let gpu = driver.gpu.as_ref().unwrap();
    let image = gpu.screen().expect("the screen was read back");
    assert_eq!(
        (image.width, image.height),
        (SIZE, SIZE),
        "the window's size"
    );
    let report = gpu.report();
    assert_eq!(report.screen_reads, 1);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn without_gpu_work_the_window_has_no_image_of_the_screen() {
    let _turn = fixture_turn();
    let driver = play_screen_fixture(GpuWork::Off);

    assert!(!driver.sprites.textures.contains_key(&SCREEN_IMAGE));
    assert_eq!(driver.screen_shown, None);
    assert_ne!(capture_of(&driver, 3).rgb(0, 0), YELLOW);
}

/// A screen image of the window's size whose top half is magenta and bottom half cyan.
fn halves() -> ScreenImage {
    let half = SIZE / 2;
    let rows = (0..SIZE).flat_map(|y| {
        let [red, green, blue] = if y < half { MAGENTA } else { CYAN };
        (0..SIZE).flat_map(move |_| [red, green, blue, 255])
    });
    ScreenImage {
        width: SIZE,
        height: SIZE,
        rgba: rows.collect(),
        frame: 0,
    }
}

#[test]
fn the_screen_image_shows_the_right_way_up_and_the_games_ui_stays_over_it() {
    let _turn = fixture_turn();
    let mut host = booted_fixture_host();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/retained-ui.fixture"))
        .unwrap();
    let window = window(GpuWork::Off, &[2]);
    let camera = viewer_options(&window).camera;
    let lighting = SceneLighting::start(&[], camera).unwrap().0;
    let mut driver = PlayerRetainedDriver::new(host, window, lighting, None).unwrap();
    driver
        .player_textures
        .set_screen_image(&halves(), &mut driver.sprites);
    let scene = SceneTables::initial_scene(None).unwrap();
    let viewer = RetainedHost::new(scene, viewer_options(&driver.window)).unwrap();

    run_frames(viewer, &mut driver, 4);

    let capture = capture_of(&driver, 2);
    assert_eq!(
        capture.rgb(2, 2),
        MAGENTA,
        "the image's top is the window's top"
    );
    assert_eq!(capture.rgb(2, 31), MAGENTA);
    assert_eq!(
        capture.rgb(2, 32),
        CYAN,
        "the image's bottom half is the window's"
    );
    assert_eq!(capture.rgb(2, 62), CYAN);
    // The game's UI (a green square at 24..32 from the bottom and a blue one at 8..16) is over
    // the image, where the stock scene's red mesh was before.
    assert_eq!(capture.rgb(28, 64 - 28), [0, 255, 0], "the green square");
    assert_eq!(capture.rgb(12, 64 - 12), [0, 0, 255], "the blue square");
}
