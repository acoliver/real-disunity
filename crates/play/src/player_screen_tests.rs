//! A headless run writes the composed frame: the screen the player's offscreen work draws into with
//! the game's UI over it. The portable screen fixture clears the screen to yellow with a command
//! buffer, and the run's PNG for a frame holds that colour at the screen's size. A second fixture
//! adds an overlay canvas with one rectangle, which the PNG shows over the yellow. Plays the
//! fixtures on a device (lavapipe in CI).

use super::*;
use crate::retained_driver::ui_tests::{fixture_config, fixture_turn};

fn fixture() -> PreparedPlayer {
    fixture_named("screen.fixture")
}

fn fixture_named(request: &str) -> PreparedPlayer {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture").join(request),
        data: tempfile::tempdir().unwrap().keep(),
        globals: Vec::new(),
    }
}

fn output(directory: &Path, frames: &[u64]) -> ScreenOutput {
    ScreenOutput {
        size: [8, 4],
        files: frames
            .iter()
            .map(|frame| (*frame, directory.join(format!("screen-{frame}.png"))))
            .collect(),
    }
}

/// The size and RGB bytes of a PNG.
fn decoded(path: &Path) -> ([u32; 2], Vec<u8>) {
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
fn a_run_writes_the_newest_screen_image_to_the_file_of_each_frame_asked_for() {
    let _turn = fixture_turn();
    let prepared = fixture();
    let directory = tempfile::tempdir().unwrap();
    let screen = output(directory.path(), &[0, 2]);

    run_player_screens(
        &prepared,
        4,
        Duration::from_millis(16),
        GpuWork::Auto,
        Some(&screen),
    )
    .unwrap();

    // The fixture clears in the work of frame 0 only, so frame 2's image is still that one.
    for (_, file) in &screen.files {
        let (size, rgb) = decoded(file);
        assert_eq!(size, [8, 4]);
        assert!(
            rgb.chunks_exact(3).all(|texel| texel == [255, 255, 0]),
            "{}: the screen is yellow all over",
            file.display()
        );
    }
    std::fs::remove_dir_all(&prepared.data).unwrap();
}

/// The first column, first row, last column and last row holding any colour but the yellow of the
/// screen, in RGB rows `width` texels wide; `None` when the image is yellow all over.
fn overlay_bounds(rgb: &[u8], width: usize) -> Option<[usize; 4]> {
    rgb.chunks_exact(3)
        .enumerate()
        .filter(|(_, texel)| *texel != [255, 255, 0])
        .map(|(index, _)| (index % width, index / width))
        .fold(None, |bounds, (x, y)| {
            Some(match bounds {
                None => [x, y, x, y],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
            })
        })
}

#[test]
fn a_headless_screen_image_has_the_overlay_canvas_over_the_game_screen() {
    let _turn = fixture_turn();
    let prepared = fixture_named("screen-ui.fixture");
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("screen-1.png");
    let screen = ScreenOutput {
        size: [64, 32],
        files: vec![(1, file.clone())],
    };

    run_player_screens(
        &prepared,
        2,
        Duration::from_millis(16),
        GpuWork::Auto,
        Some(&screen),
    )
    .unwrap();

    // The canvas centre is the centre of the 64 by 32 screen, (32, 16). The fixture's rectangle
    // is 16 pixels square with its lower left corner 24 pixels left of the centre, on the
    // centre's row, so it covers columns 8 to 23 of the top 16 rows; the rest is the yellow
    // screen.
    let (size, rgb) = decoded(&file);
    assert_eq!(size, [64, 32]);
    assert_eq!(
        overlay_bounds(&rgb, 64),
        Some([8, 0, 23, 15]),
        "the first column, first row, last column and last row the overlay covers"
    );
    for (index, texel) in rgb.chunks_exact(3).enumerate() {
        let (x, y) = (index % 64, index / 64);
        let inside = (8..24).contains(&x) && y < 16;
        let expected = if inside { [0, 255, 0] } else { [255, 255, 0] };
        assert_eq!(texel, expected, "pixel {x},{y}");
    }
    std::fs::remove_dir_all(&prepared.data).unwrap();
}

#[test]
fn a_run_without_a_screen_writes_nothing_and_a_frame_past_the_run_is_refused() {
    let _turn = fixture_turn();
    let prepared = fixture();
    let directory = tempfile::tempdir().unwrap();
    let past = output(directory.path(), &[1, 4]);

    let error = run_player_screens(
        &prepared,
        4,
        Duration::from_millis(16),
        GpuWork::Auto,
        Some(&past),
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("screen frame 4 is not among the 4 frames"),
        "{error}"
    );
    run_player_with(&prepared, 2, Duration::from_millis(16), GpuWork::Auto).unwrap();
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none(),
        "no file was written"
    );
    std::fs::remove_dir_all(&prepared.data).unwrap();
}

#[test]
fn a_screen_needs_the_offscreen_work_on_a_device() {
    let screen = output(Path::new("."), &[0]);
    let error = check_screen(None, 1, Some(&screen)).unwrap_err();
    assert!(
        error.to_string().contains("needs the offscreen work"),
        "{error}"
    );
    assert!(check_screen(None, 1, None).is_ok());
}

#[test]
fn a_frame_asked_for_before_any_work_drew_into_the_screen_is_an_error_naming_the_frame() {
    let screen = output(Path::new("."), &[3]);
    assert!(
        screens_due(None, 2, Some(&screen)).is_ok(),
        "an unlisted frame"
    );
    assert!(screens_due(None, 3, None).is_ok(), "no screen");
    let error = screens_due(None, 3, Some(&screen)).unwrap_err();
    assert!(
        error.to_string().contains("no screen image after frame 3"),
        "{error}"
    );
}
