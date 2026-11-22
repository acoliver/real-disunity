//! The windowed player shows the game's own frame: the screen texture the player's GPU work drew
//! into covers the stock scene, with no copy through the CPU.
//!
//! The tests play the portable screen fixture, whose script clears the screen to yellow with a
//! command buffer, so the window shows that frame. They need a GPU adapter (lavapipe in CI), and
//! the colours use the channel values 0 and 1, which stay the same through every colour-space encoding. Which way up
//! the screen shows and that the UI draws over it are the viewer's and compositor's tests.

use super::ui_tests::{fixture_config, fixture_turn};
use super::*;
use std::{path::PathBuf, time::Instant};

use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_scene_viewer::SharedGpu;

use crate::GpuLaunch;
use crate::PreparedPlayer;
use crate::gpu_device::headless::HeadlessViewer;
use crate::player::window_gpu;

const SIZE: u32 = 64;
const YELLOW: [u8; 3] = [255, 255, 0];

fn window(capture_frames: &[u64]) -> PlayerWindow {
    PlayerWindow {
        title: "windowless screen".into(),
        size: [SIZE, SIZE],
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames: None,
        capture_frames: capture_frames.iter().copied().collect(),
        keys: vec![],
    }
}

/// Runs `frames` updates of `driver` in the windowless viewer, on `gpu`'s device, and waits for
/// its captures. Returns how many submissions each update's frame made through `authority`, the
/// authority of that device.
fn run_frames(
    viewer: RetainedHost,
    driver: &mut PlayerRetainedDriver,
    frames: u64,
    (gpu, authority): (SharedGpu, SubmissionAuthority),
) -> Vec<u64> {
    let mut run = viewer.windowless(driver, Some(gpu)).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut submissions = Vec::new();
    for _ in 0..frames {
        let before = authority.submitted_serial();
        while !run.frame().unwrap() {
            assert!(Instant::now() < deadline, "frame admission timed out");
            run.poll().unwrap();
            std::thread::yield_now();
        }
        run.poll().unwrap();
        submissions.push(authority.submitted_serial() - before);
    }
    while run.poll().unwrap() {
        assert!(Instant::now() < deadline, "capture timed out");
        std::thread::yield_now();
    }
    run.shutdown().unwrap();
    submissions
}

fn capture_of(driver: &PlayerRetainedDriver, frame: u64) -> &Capture {
    driver
        .captures
        .iter()
        .find(|capture| capture.frame == frame)
        .unwrap_or_else(|| panic!("no capture of frame {frame}"))
}

/// Plays the screen fixture in a window for four updates, capturing the first and last. Returns the
/// driver after the run and the submissions each update made.
fn play_screen_fixture() -> (PlayerRetainedDriver, Vec<u64>) {
    play_fixture("screen.fixture")
}

fn play_fixture(request: &str) -> (PlayerRetainedDriver, Vec<u64>) {
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture").join(request),
        data: data.path().to_owned(),
        gpu: GpuLaunch::default(),
    };
    let mut source = HeadlessViewer::new([SIZE, SIZE]);
    let gpu = window_gpu(&prepared.data, &prepared.gpu, &mut source)
        .expect("the test needs a GPU adapter");
    let authority = source.authority();
    let (viewer, mut driver) = start_window(&prepared, window(&[0, 3]), gpu).unwrap();
    let submissions = run_frames(viewer, &mut driver, 4, (source.gpu, authority));
    (driver, submissions)
}

#[test]
fn the_window_shows_the_screen_a_script_cleared_over_the_stock_scene() {
    let _turn = fixture_turn();
    let (driver, submissions) = play_screen_fixture();

    for frame in [0, 3] {
        let capture = capture_of(&driver, frame);
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(capture.rgb(x, y), YELLOW, "frame {frame} at {x},{y}");
            }
        }
    }
    let gpu = &driver.gpu;
    let screen = gpu
        .game_screen()
        .expect("the screen is offered to the viewer");
    assert_eq!(
        (screen.texture().width(), screen.texture().height()),
        (SIZE, SIZE),
        "the window's size"
    );
    assert!(gpu.screen().is_none(), "no image of the screen was made");
    let report = gpu.report();
    assert_eq!(report.screen_reads, 0, "a window frame reads nothing back");
    assert!(report.screen_reads_refused.is_empty());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(
        submissions[0], submissions[2],
        "the update that carries the script's clears is one submission, as an update with no work is"
    );
}

#[test]
fn first_update_after_resize_flushes_into_resized_screen() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture/resize-flush.fixture"),
        data: data.path().to_owned(),
        gpu: GpuLaunch::default(),
    };
    let mut source = HeadlessViewer::new([SIZE, SIZE]);
    let gpu = window_gpu(&prepared.data, &prepared.gpu, &mut source)
        .expect("the test needs a GPU adapter");
    let (mut viewer, mut driver) = start_window(&prepared, window(&[]), gpu).unwrap();

    let resized = [SIZE * 2, SIZE];
    viewer.input(RetainedInput::Resized(resized));
    driver
        .step(
            &mut viewer,
            &RetainedTick {
                frame: 0,
                elapsed: Duration::ZERO,
                dt: Duration::from_millis(16),
            },
        )
        .unwrap();

    let screen = driver
        .gpu
        .game_screen()
        .expect("the first update's readback flush must write the resized screen");
    assert_eq!(
        (screen.texture().width(), screen.texture().height()),
        (resized[0], resized[1]),
        "the first update after resize must offer a screen at the current window size"
    );
    assert!(
        screen.submitted(),
        "the managed readback flushed the screen write"
    );
}

#[test]
fn a_canvas_quad_over_a_render_texture_a_script_cleared_shows_it_in_the_window() {
    let _turn = fixture_turn();
    let (driver, _) = play_fixture("ui-target-draw.fixture");

    let shown = capture_of(&driver, 3);
    assert_eq!(
        shown.rgb(32, 32),
        YELLOW,
        "the quad shows the cleared texture"
    );
    assert_eq!(shown.rgb(18, 46), YELLOW, "the quad's corner");
    assert_ne!(
        shown.rgb(4, 4),
        YELLOW,
        "outside the quad is the stock scene"
    );
    let report = driver.gpu.report();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}
