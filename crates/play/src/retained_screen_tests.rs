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
use worldrunner_scene_viewer::{RetainedInput, SharedGpu};

use crate::GpuLaunch;
use crate::PreparedPlayer;
use crate::gpu_device::{SharedDevice, headless};
use crate::player::window_gpu;

const SIZE: u32 = 64;
/// The game frames a window of the screen fixture plays, the last of which is captured.
const FRAMES: u64 = 4;
const YELLOW: [u8; 3] = [255, 255, 0];

fn window(capture_frames: &[u64]) -> PlayerWindow {
    PlayerWindow {
        title: "windowless screen".into(),
        size: [SIZE, SIZE],
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames: Some(FRAMES),
        capture_frames: capture_frames.iter().copied().collect(),
        capture_files: Vec::new(),
        keys: vec![],
    }
}

/// Runs `driver` in the windowless viewer, on `gpu`'s device, until its script closes the
/// session. Returns how many submissions each submitted frame made through `authority`, the
/// authority of that device.
fn run_frames(
    viewer: RetainedHost,
    driver: &mut PlayerRetainedDriver,
    (gpu, authority): (SharedGpu, SubmissionAuthority),
) -> Vec<u64> {
    let mut run = viewer.windowless(driver, Some(gpu)).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut submissions = Vec::new();
    while !run.is_closed() {
        let before = authority.submitted_serial();
        while !run.is_closed() && !run.frame().unwrap() {
            assert!(Instant::now() < deadline, "frame admission timed out");
            run.poll().unwrap();
            std::thread::yield_now();
        }
        if !run.is_closed() {
            run.poll().unwrap();
        }
        submissions.push(authority.submitted_serial() - before);
        assert!(Instant::now() < deadline, "the run timed out");
    }
    run.shutdown().unwrap();
    submissions
}

fn capture_of(captures: &[Capture], frame: u64) -> &Capture {
    captures
        .iter()
        .find(|capture| capture.frame == frame)
        .unwrap_or_else(|| panic!("no capture of frame {frame}"))
}

/// Plays the screen fixture in a window for four updates, capturing the first and last. Returns the
/// driver after the run and the submissions each update made.
fn play_screen_fixture() -> (PlayerRetainedDriver, Vec<u64>) {
    play_fixture("screen.fixture")
}

type Started = (
    RetainedHost,
    PlayerRetainedDriver,
    (SharedGpu, SubmissionAuthority),
);

/// The viewer and driver that play `request`, a portable fixture, in `window`, with the GPU they
/// run on.
fn start_fixture(request: &str, window: &PlayerWindow) -> Started {
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture").join(request),
        data: data.path().to_owned(),
        screen: None,
        gpu: GpuLaunch::default(),
    };
    let (mut shared, options) = headless::shared([SIZE, SIZE]);
    let gpu = window_gpu(
        &prepared.data,
        &prepared.gpu,
        &mut SharedDevice {
            gpu: &mut shared,
            options: &options,
        },
    )
    .expect("the test needs a GPU adapter");
    let authority = headless::authority(&mut shared, &options);
    let (viewer, driver) = start_window(&prepared, window, options, gpu).unwrap();
    (viewer, driver, (shared, authority))
}

fn play_fixture(request: &str) -> (PlayerRetainedDriver, Vec<u64>) {
    let (viewer, mut driver, gpu) = start_fixture(request, &window(&[0, 3]));
    let submissions = run_frames(viewer, &mut driver, gpu);
    (driver, submissions)
}

#[test]
fn the_window_shows_the_screen_a_script_cleared_over_the_stock_scene() {
    let _turn = fixture_turn();
    let (mut driver, submissions) = play_screen_fixture();
    let captures = driver.script.finish().unwrap();

    for frame in [0, 3] {
        let capture = capture_of(&captures, frame);
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
    assert_eq!(
        gpu.drawn_screen(),
        Some(0),
        "the update that carries the script's clears drew into the screen"
    );
    let report = gpu.report();
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
        screen: None,
        gpu: GpuLaunch::default(),
    };
    let (mut shared, options) = headless::shared([SIZE, SIZE]);
    let gpu = window_gpu(
        &prepared.data,
        &prepared.gpu,
        &mut SharedDevice {
            gpu: &mut shared,
            options: &options,
        },
    )
    .expect("the test needs a GPU adapter");
    let (mut viewer, mut driver) = start_window(&prepared, &window(&[]), options, gpu).unwrap();

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
    let (mut driver, _) = play_fixture("ui-target-draw.fixture");
    let captures = driver.script.finish().unwrap();

    let shown = capture_of(&captures, 3);
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

#[test]
fn a_capture_is_in_its_file_once_its_frame_is_delivered_and_before_the_run_ends() {
    let _turn = fixture_turn();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("frame0.png");
    let mut asked = window(&[]);
    asked.capture_files = vec![(0, file.clone())];
    let (viewer, mut driver, (gpu, _)) = start_fixture("screen.fixture", &asked);
    let mut run = viewer.windowless(&mut driver, Some(gpu)).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut written_while_running = false;

    while !run.is_closed() {
        if !run.frame().unwrap() {
            std::thread::yield_now();
        }
        if !run.is_closed() {
            run.poll().unwrap();
        }
        written_while_running |= !run.is_closed() && file.exists();
        assert!(Instant::now() < deadline, "the run timed out");
    }
    run.shutdown().unwrap();

    assert!(
        written_while_running,
        "the file of frame 0 appeared only when the run of {FRAMES} frames ended"
    );
    assert!(file.exists());
}
