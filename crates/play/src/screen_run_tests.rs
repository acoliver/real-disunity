//! A windowless run plays a built player through the retained driver, with no display, and returns
//! the frames it was asked for. Frame N is the image after game frame N with the UI of frame N over
//! the game's screen. The portable fixtures clear the screen with command buffers and draw one
//! overlay rectangle, and the colours use only the channel values 0 and 255, so a frame that shows
//! a neighbour's update cannot hide behind a colour-space encoding. They need a GPU adapter
//! (lavapipe in CI) and .NET.

use super::ui_tests::{fixture_config, fixture_turn};
use super::*;
use crate::GpuLaunch;

const YELLOW: [u8; 3] = [255, 255, 0];
const CYAN: [u8; 3] = [0, 255, 255];
const GREEN: [u8; 3] = [0, 255, 0];

fn prepared(request: &str) -> (PreparedPlayer, tempfile::TempDir) {
    let data = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture").join(request),
        data: data.path().to_owned(),
        screen: None,
        gpu: GpuLaunch::default(),
    };
    (prepared, data)
}

fn window(size: [u32; 2], max_frames: Option<u64>, captures: &[u64]) -> PlayerWindow {
    PlayerWindow {
        title: "windowless run".into(),
        size,
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames,
        capture_frames: captures.iter().copied().collect(),
        capture_files: Vec::new(),
        keys: vec![],
    }
}

fn capture_of(captures: &[Capture], frame: u64) -> &Capture {
    captures
        .iter()
        .find(|capture| capture.frame == frame)
        .unwrap_or_else(|| panic!("no capture of frame {frame}"))
}

fn frames(captures: &[Capture]) -> Vec<u64> {
    let mut frames: Vec<u64> = captures.iter().map(|capture| capture.frame).collect();
    frames.sort_unstable();
    frames
}

fn assert_all_pixels(capture: &Capture, expected: [u8; 3]) {
    for y in 0..capture.height {
        for x in 0..capture.width {
            assert_eq!(
                capture.rgb(x, y),
                expected,
                "frame {} at {x},{y}",
                capture.frame
            );
        }
    }
}

/// The first column, first row, last column and last row of the pixels that are `colour`; `None`
/// when no pixel is.
fn bounds_of(capture: &Capture, colour: [u8; 3]) -> Option<[u32; 4]> {
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..capture.height {
        for x in 0..capture.width {
            if capture.rgb(x, y) == colour {
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
            }
        }
    }
    bounds
}

#[test]
fn a_windowless_run_gives_the_newest_screen_image_at_each_frame_asked_for() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("screen.fixture");

    let captures = play_player_windowless(&prepared, &window([8, 4], Some(4), &[0, 2])).unwrap();

    // The fixture clears in the work of frame 0 only, so frame 2's image is still that one.
    assert_eq!(frames(&captures), [0, 2]);
    for capture in &captures {
        assert_eq!((capture.width, capture.height), (8, 4));
        assert_all_pixels(capture, YELLOW);
    }
}

#[test]
fn a_windowless_capture_has_the_overlay_canvas_over_the_game_screen() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("screen-ui.fixture");

    let captures = play_player_windowless(&prepared, &window([64, 32], Some(2), &[1])).unwrap();

    // The canvas centre is the centre of the 64 by 32 screen, (32, 16). The fixture's rectangle
    // is 16 pixels square with its lower left corner 24 pixels left of the centre, on the
    // centre's row, so it covers columns 8 to 23 of the top 16 rows; the rest is the yellow
    // screen.
    let capture = capture_of(&captures, 1);
    assert_eq!((capture.width, capture.height), (64, 32));
    assert_eq!(bounds_of(capture, GREEN), Some([8, 0, 23, 15]));
    for y in 0..32 {
        for x in 0..64 {
            let inside = (8..24).contains(&x) && y < 16;
            let expected = if inside { GREEN } else { YELLOW };
            assert_eq!(capture.rgb(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_capture_before_any_work_drew_the_screen_is_an_error_naming_the_frame() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("retained-ui.fixture");

    let error = play_player_windowless(&prepared, &window([64, 64], Some(2), &[0]))
        .expect_err("the run is refused");

    assert!(
        error
            .to_string()
            .contains("no screen image after frame 0: no work has drawn into the screen yet"),
        "{error}"
    );
}

#[test]
fn every_capture_shows_the_screen_and_the_overlay_of_its_own_game_frame() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("screen-count.fixture");

    let captures =
        play_player_windowless(&prepared, &window([64, 32], Some(5), &[1, 2, 3])).unwrap();

    // Game frame N clears the screen yellow when N is even and cyan when it is odd, and puts the
    // overlay rectangle 8 * N columns right of columns 8 to 23.
    assert_eq!(frames(&captures), [1, 2, 3]);
    for (frame, screen) in [(1, CYAN), (2, YELLOW), (3, CYAN)] {
        let capture = capture_of(&captures, frame);
        let left = 8 + 8 * u32::try_from(frame).unwrap();
        assert_eq!(capture.rgb(0, 31), screen, "frame {frame}: the screen");
        assert_eq!(
            bounds_of(capture, GREEN),
            Some([left, 0, left + 15, 15]),
            "frame {frame}: the overlay"
        );
    }
}

#[test]
fn a_frame_past_the_run_is_refused_before_the_scene_starts() {
    let error = window([8, 4], Some(4), &[1, 4]).validate().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("screen frame 4 is not among the 4 frames the run plays"),
        "{error}"
    );
}

#[test]
fn the_last_frame_of_the_run_and_a_run_with_no_end_pass_the_check() {
    assert!(window([8, 4], Some(4), &[0, 3]).validate().is_ok());
    assert!(window([8, 4], None, &[0, 900]).validate().is_ok());
}

#[test]
fn a_run_with_no_viewer_advances_the_fixture_without_a_screen() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("screen.fixture");

    crate::run_player(&prepared, 2, Duration::from_millis(16)).unwrap();
}

#[test]
fn a_frame_the_gpu_refuses_ends_the_run_and_no_screen_is_written_for_it_or_after() {
    let _turn = fixture_turn();
    let (prepared, _data) = prepared("screen-refused.fixture");
    let directory = tempfile::tempdir().unwrap();
    let mut window = window([8, 4], Some(4), &[]);
    window.capture_files = (0..4)
        .map(|frame| (frame, directory.path().join(format!("screen-{frame}.png"))))
        .collect();

    let error = play_player_windowless(&prepared, &window).expect_err("the run is refused");

    // Frame 0 drew the yellow; the clear of frame 1 needs 256 GiB, which no budget has.
    let message = error.to_string();
    assert!(message.contains("frame 1"), "{message}");
    assert!(message.contains("refused by the GPU"), "{message}");
    assert!(message.contains("over its budget"), "{message}");
    let mut written: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    written.sort();
    assert_eq!(written, ["screen-0.png"], "{message}");
}

/// The size and RGB bytes of a PNG.
fn decoded(path: &std::path::Path) -> ([u32; 2], Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    bytes.truncate(info.buffer_size());
    ([info.width, info.height], bytes)
}

#[test]
fn a_run_with_no_viewer_writes_the_target_of_the_record_it_is_asked_for() {
    let _turn = fixture_turn();
    let (mut prepared, _data) = prepared("screen.fixture");
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("record.png");
    prepared.screen = Some([8, 4]);
    prepared.gpu.reads = vec![crate::TargetRead {
        frame: 0,
        record: 0,
        file: file.clone(),
    }];

    crate::run_player(&prepared, 2, Duration::from_millis(16)).unwrap();

    // The fixture's first record clears the screen to yellow.
    let (size, bytes) = decoded(&file);
    assert_eq!(size, [8, 4]);
    assert!(
        bytes.chunks_exact(3).all(|texel| texel == YELLOW),
        "{bytes:?}"
    );
}

#[test]
fn a_read_of_a_frame_the_run_does_not_play_is_refused_before_the_scene_starts() {
    let _turn = fixture_turn();
    let (mut prepared, _data) = prepared("screen.fixture");
    prepared.gpu.reads = vec![crate::TargetRead {
        frame: 2,
        record: 0,
        file: "never.png".into(),
    }];

    let error = crate::run_player(&prepared, 2, Duration::from_millis(16))
        .expect_err("the read names a frame past the run");

    assert!(
        error
            .to_string()
            .contains("--read-target frame 2 is not among the 2 frames the run plays"),
        "{error}"
    );
}
