//! A headless run writes the screen the player's offscreen work draws into: the portable screen
//! fixture clears the screen to yellow with a command buffer, and the run's PNG for a frame holds
//! that colour at the screen's size. Plays the fixture on a device (lavapipe in CI).

use super::*;
use crate::retained_driver::ui_tests::{fixture_config, fixture_turn};

fn fixture() -> PreparedPlayer {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    PreparedPlayer {
        config: fixture_config(),
        request: root.join("csharp/PortableFixture/screen.fixture"),
        data: tempfile::tempdir().unwrap().keep(),
        depth: DepthRange::default(),
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
        write_screens(None, 2, Some(&screen)).is_ok(),
        "an unlisted frame"
    );
    assert!(write_screens(None, 3, None).is_ok(), "no screen");
    let error = write_screens(None, 3, Some(&screen)).unwrap_err();
    assert!(
        error.to_string().contains("no screen image after frame 3"),
        "{error}"
    );
}
