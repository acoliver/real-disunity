//! The `--read-target` option of the command line.

use super::{Arguments, check_reads, parse};

fn parsed(args: &[&str]) -> Result<Arguments, String> {
    parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn repeated_read_targets_keep_their_order_and_a_windows_path_keeps_its_colon() {
    let arguments = parsed(&[
        "--player",
        "Game_Data",
        "--read-target",
        "42:17:out/a.png",
        "--read-target",
        "42:3:C:\\out\\b.png",
    ])
    .expect("parses");
    let reads: Vec<_> = arguments
        .reads
        .iter()
        .map(|read| (read.frame, read.record, read.file.to_str().unwrap()))
        .collect();
    assert_eq!(reads, [(42, 17, "out/a.png"), (42, 3, "C:\\out\\b.png")]);
}

#[test]
fn a_malformed_read_target_names_the_option_the_text_and_the_fault() {
    for (text, fault) in [
        ("a.png", "is not FRAME:RECORD:FILE.png"),
        ("1:a.png", "is not FRAME:RECORD:FILE.png"),
        ("x:1:a.png", "frame `x` is not a number"),
        ("1:y:a.png", "record `y` is not a number"),
        ("1:2:a.jpg", "FILE must end in .png"),
        ("1:2:.png", "FILE must end in .png"),
    ] {
        let error = parsed(&["--player", "Game_Data", "--read-target", text])
            .err()
            .expect(text);
        assert!(
            error.contains("--read-target") && error.contains(text) && error.contains(fault),
            "{error}"
        );
    }
}

#[test]
fn a_record_or_a_file_given_twice_is_refused() {
    let twice = |second: &str| {
        parsed(&[
            "--player",
            "Game_Data",
            "--read-target",
            "1:2:a.png",
            "--read-target",
            second,
        ])
        .err()
    };
    assert!(
        twice("1:2:b.png")
            .expect("same record")
            .contains("given twice")
    );
    assert!(
        twice("3:4:a.png")
            .expect("same file")
            .contains("given twice")
    );
    assert!(twice("1:3:b.png").is_none());
}

fn read_checked(args: &[&str]) -> Result<(), String> {
    check_reads(&parsed(args).expect("parses"))
}

#[test]
fn a_read_target_belongs_to_a_headless_player_run_that_reaches_its_frame() {
    let read = ["--player", "Game_Data", "--read-target", "1:2:a.png"];
    assert!(read_checked(&[&read[..], &["--frames", "2"]].concat()).is_ok());
    assert!(
        read_checked(&read).is_err(),
        "frame 1 is not among the 1 frame run"
    );
    assert!(read_checked(&[&read[..], &["--frames", "1"]].concat()).is_err());
    assert!(
        read_checked(&["snake", "--read-target", "0:2:a.png"]).is_err(),
        "not a game's"
    );
    for window in [
        ["--capture", "0:w.png"],
        ["--key", "0:w:down"],
        ["--screen", "0:s.png"],
    ] {
        assert!(
            read_checked(&[&read[..], &["--frames", "2"], &window[..]].concat()).is_err(),
            "{window:?}"
        );
    }
}
