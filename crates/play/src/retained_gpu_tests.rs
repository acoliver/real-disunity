//! The windowed player does the player's offscreen work: a script that blits a colour into a
//! render texture and reads it back into a `Texture2D` shown in a UI image fills the image in
//! the window, as it does in a headless run.
//!
//! The portable readback fixture reads its first colour while the scene loads and its second in a
//! later `Update`, through a plain blit (no material), so no shader is involved and the expected
//! colours are the script's colours. Their channels are 0 or 1, which stay the same through the
//! colour-space encodings between the blit and the captured frame. The window is the windowless
//! viewer of the other retained tests and needs a GPU adapter (lavapipe in CI).

use super::ui_tests::fixture_config;
use super::ui_tests::fixture_turn;
use super::*;
use std::path::PathBuf;
use std::time::Instant;

use crate::GpuLaunch;
use crate::PreparedPlayer;
use crate::gpu_device::{SharedDevice, headless};
use crate::player::window_gpu;

const SIZE: u32 = 64;
/// The update that reads the second colour is the fourth `Update` of the script: one in the
/// scene's first step and one for each of the window's first three frames.
const SECOND_COLOR_FRAME: u64 = 2;
const FIRST: [u8; 3] = [255, 255, 0];
const SECOND: [u8; 3] = [0, 255, 255];
const UPDATES: u64 = 8;

/// What a windowless run of the readback scene captured and what its GPU work reported.
struct Played {
    captures: Vec<Capture>,
    frames: u64,
    pixel_reads: usize,
    errors: Vec<String>,
}

fn readback_window() -> PlayerWindow {
    PlayerWindow {
        title: "windowless readback".into(),
        size: [SIZE, SIZE],
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames: Some(UPDATES),
        capture_frames: BTreeSet::from([0, SECOND_COLOR_FRAME + 4]),
        capture_files: Vec::new(),
        keys: vec![],
    }
}

/// Plays the readback scene through the code `play_player_window` runs to start.
fn play() -> Played {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture/readback.fixture"),
        data: data.path().to_owned(),
        screen: None,
        gpu: GpuLaunch::default(),
    };
    let window = readback_window();
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
    let (viewer, mut driver) = start_window(&prepared, &window, options, gpu).unwrap();
    let mut run = viewer.windowless(&mut driver, Some(shared)).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    while !run.is_closed() {
        assert!(Instant::now() < deadline, "the run timed out");
        run.frame().unwrap();
        if !run.is_closed() {
            run.poll().unwrap();
        }
        std::thread::yield_now();
    }
    run.shutdown().unwrap();
    drop(run);
    let report = driver.gpu.report();
    Played {
        frames: report.frames,
        pixel_reads: report.pixel_reads,
        errors: report.errors.keys().cloned().collect(),
        captures: driver.script.finish().unwrap(),
    }
}

fn pixel(image: &Capture, x: u32, y: u32) -> [u8; 4] {
    let offset = ((image.height - 1 - y) * image.width + x) as usize * 4;
    image.pixels[offset..offset + 4].try_into().unwrap()
}

/// Whether every pixel of the 24 by 24 square inside the 32 by 32 image is `color`, within 3 of
/// each 8-bit channel.
fn image_shows(image: &Capture, color: [u8; 3]) -> bool {
    (20..44).all(|y| {
        (20..44).all(|x| {
            let shown = pixel(image, x, y);
            (0..3).all(|channel| shown[channel].abs_diff(color[channel]) <= 3)
        })
    })
}

fn capture_of(played: &Played, frame: u64) -> &Capture {
    played
        .captures
        .iter()
        .find(|capture| capture.frame == frame)
        .unwrap_or_else(|| panic!("no capture of frame {frame}"))
}

#[test]
fn a_blit_read_back_into_a_texture_shows_in_the_window_and_a_later_readback_replaces_it() {
    let played = play();
    assert_eq!(played.captures.len(), 2);
    let first = capture_of(&played, 0);
    let later = capture_of(&played, SECOND_COLOR_FRAME + 4);
    assert!(
        image_shows(first, FIRST),
        "the colour read while the scene loaded: {:?}",
        pixel(first, 32, 32)
    );
    assert!(
        image_shows(later, SECOND),
        "the colour read in a later update: {:?}",
        pixel(later, 32, 32)
    );
    assert!(
        !image_shows(first, SECOND) && !image_shows(later, FIRST),
        "each capture shows its own colour"
    );
    assert_ne!(
        pixel(first, 2, 2)[..3],
        FIRST,
        "outside the image is not the colour"
    );
    assert!(played.errors.is_empty(), "{:?}", played.errors);
    assert_eq!(played.pixel_reads, 2, "the two readbacks were answered");
    assert_eq!(
        played.frames, UPDATES,
        "the window runs the GPU frame path once per update"
    );
}
