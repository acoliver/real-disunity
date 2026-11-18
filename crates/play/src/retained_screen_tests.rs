//! The windowed player shows the game's own frame: the screen texture the player's GPU work drew
//! into covers the stock scene, with no copy through the CPU.
//!
//! The tests play the portable screen fixture, whose script clears the screen to yellow with a
//! command buffer, so the window shows that frame with the offscreen work on a device, and the
//! same run without GPU work. They need a GPU adapter (lavapipe in CI), and the colours use the
//! channel values 0 and 1, which stay the same through every colour-space encoding. Which way up
//! the screen shows and that the UI draws over it are the viewer's and compositor's tests.

use super::ui_tests::{fixture_config, fixture_turn};
use super::*;
use std::{path::PathBuf, time::Instant};

use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_scene_viewer::SharedGpu;

use crate::PreparedPlayer;
use crate::gpu_device::headless::HeadlessViewer;
use crate::player::window_gpu;

const SIZE: u32 = 64;
const YELLOW: [u8; 3] = [255, 255, 0];

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

/// Runs `frames` updates of `driver` in the windowless viewer, on `shared`'s device when the
/// player works on one, and waits for its captures. Returns how many submissions each update's
/// frame made through the authority of that device.
fn run_frames(
    viewer: RetainedHost,
    driver: &mut PlayerRetainedDriver,
    frames: u64,
    shared: Option<(SharedGpu, SubmissionAuthority)>,
) -> Vec<u64> {
    let (gpu, authority) = shared.unzip();
    let mut run = match gpu {
        Some(gpu) => viewer.windowless_on(driver, gpu),
        None => viewer.windowless(driver),
    }
    .unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut submissions = Vec::new();
    for _ in 0..frames {
        let before = authority
            .as_ref()
            .map(SubmissionAuthority::submitted_serial);
        while !run.frame().unwrap() {
            assert!(Instant::now() < deadline, "frame admission timed out");
            run.poll().unwrap();
            std::thread::yield_now();
        }
        run.poll().unwrap();
        submissions.extend(
            before
                .zip(authority.as_ref())
                .map(|(before, authority)| authority.submitted_serial() - before),
        );
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

/// Plays the screen fixture in a window for four updates, capturing the first and last, with the GPU work
/// the window asks for. Returns the driver after the run and the submissions each update made.
fn play_screen_fixture(gpu_work: GpuWork) -> (PlayerRetainedDriver, Vec<u64>) {
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture/screen.fixture"),
        data: data.path().to_owned(),
    };
    let mut source = HeadlessViewer::new([SIZE, SIZE]);
    let gpu = window_gpu(gpu_work, &prepared.data, &mut source);
    assert_eq!(
        gpu.is_some(),
        gpu_work == GpuWork::Auto,
        "the test needs a GPU adapter"
    );
    let shared = gpu.is_some().then(|| (source.authority(), source.gpu));
    let (viewer, mut driver) = start_window(&prepared, window(gpu_work, &[0, 3]), gpu).unwrap();
    let submissions = run_frames(
        viewer,
        &mut driver,
        4,
        shared.map(|(authority, gpu)| (gpu, authority)),
    );
    (driver, submissions)
}

#[test]
fn the_window_shows_the_screen_a_script_cleared_over_the_stock_scene() {
    let _turn = fixture_turn();
    let (driver, submissions) = play_screen_fixture(GpuWork::Auto);

    for frame in [0, 3] {
        let capture = capture_of(&driver, frame);
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(capture.rgb(x, y), YELLOW, "frame {frame} at {x},{y}");
            }
        }
    }
    let gpu = driver.gpu.as_ref().unwrap();
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
    };
    let mut source = HeadlessViewer::new([SIZE, SIZE]);
    let gpu = window_gpu(GpuWork::Auto, &prepared.data, &mut source);
    assert!(gpu.is_some(), "the test needs a GPU adapter");
    let (mut viewer, mut driver) =
        start_window(&prepared, window(GpuWork::Auto, &[]), gpu).unwrap();

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
        .as_ref()
        .unwrap()
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
fn without_gpu_work_the_window_has_no_image_of_the_screen() {
    let _turn = fixture_turn();
    let (driver, _) = play_screen_fixture(GpuWork::Off);

    assert!(driver.gpu.is_none());
    assert_ne!(capture_of(&driver, 3).rgb(0, 0), YELLOW);
}
