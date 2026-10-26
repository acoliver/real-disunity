//! Plays Pong, Breakout and Snake in a real window and checks the captured frames.
//!
//! Each game runs in its own process, as one window per process is the desktop host's rule. The
//! child processes drop `WAYLAND_DISPLAY` so the window opens on the X display `xvfb-run`
//! provides, never on a developer's Wayland session, where an unseen window is never presented.
//! Pixel positions follow from each game's camera at the 480x270 window size used here.

#[cfg(target_os = "linux")]
mod scenarios {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Duration;

    use worldrunner_play::{Capture, Game, PlayDriver, prepare};
    use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

    const SIZE: [u32; 2] = [480, 270];
    const SEED: i32 = 77;

    pub fn run() {
        assert!(std::env::var_os("DISPLAY").is_some(), "run under xvfb-run");
        if let Some(case) =
            std::env::args().find_map(|arg| arg.strip_prefix("--case=").map(str::to_owned))
        {
            match case.as_str() {
                "pong" => pong(),
                "breakout" => breakout(),
                "snake" => snake(),
                other => panic!("unknown case {other}"),
            }
            return;
        }
        let library = engine_library();
        for case in ["pong", "breakout", "snake"] {
            let status = Command::new(std::env::current_exe().unwrap())
                .arg(format!("--case={case}"))
                .env_remove("WAYLAND_DISPLAY")
                .env("DISUNITY_FFI_LIBRARY", &library)
                .status()
                .unwrap();
            assert!(status.success(), "{case} failed: {status}");
        }
    }

    /// Builds the engine library the games load, as the managed tests do.
    fn engine_library() -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--quiet",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
            ])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(status.success(), "building the engine library failed");
        let target =
            std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
        let target = if target.is_absolute() {
            target
        } else {
            root.join(target)
        };
        target.join("debug").join(format!(
            "{}worldrunner_disunity_ffi{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ))
    }

    fn key(frame: u64, key: &str, pressed: bool) -> ScheduledInput {
        let event = LiveInputEvent::Key {
            key: key.into(),
            pressed,
            repeat: false,
        };
        ScheduledInput { frame, event }
    }

    /// Plays `game` for `frames` fixed 20 ms frames with `input`, returning the captured frames
    /// in order.
    fn play(game: Game, frames: u64, input: Vec<ScheduledInput>, captures: &[u64]) -> Vec<Capture> {
        // A fixed seed fixes Pong's random launch direction, so every run plays the same rally.
        let prepared = prepare(game, None, Some(SEED)).unwrap();
        let mut driver = PlayDriver::new(prepared, captures.iter().copied()).unwrap();
        let mut options = LiveOptions::new(format!("play {}", game.name()), SIZE);
        options.fixed_dt = Some(Duration::from_millis(20));
        options.max_frames = Some(frames);
        options.scheduled_input = input;
        run_live(&mut driver, options).unwrap();
        let presented: Vec<u64> = driver
            .captures()
            .iter()
            .map(|capture| capture.frame)
            .collect();
        assert_eq!(
            presented, captures,
            "every requested frame is presented and captured"
        );
        driver.captures().to_vec()
    }

    fn white(capture: &Capture, x: u32, y: u32) -> bool {
        capture.rgb(x, y).iter().all(|&channel| channel > 200)
    }

    /// The first and last rows with white pixels in columns `x`, if any.
    fn white_rows(
        capture: &Capture,
        x: std::ops::Range<u32>,
        y: std::ops::Range<u32>,
    ) -> Option<(u32, u32)> {
        let rows: Vec<u32> = y
            .filter(|&row| x.clone().any(|column| white(capture, column, row)))
            .collect();
        Some((*rows.first()?, *rows.last()?))
    }

    /// Pong's camera shows 16 units over 270 rows, so the paddles at x -10 and 10 cover
    /// columns around 71 and 409, and the scores are drawn above row 40. With the fixed seed the
    /// ball launches toward the left goal and passes the raised paddle.
    fn pong() {
        let frames = play(
            Game::Pong,
            61,
            vec![key(10, "w", true), key(40, "w", false)],
            &[0, 30, 60],
        );
        let (left, right, field) = (65..78, 403..416, 0..SIZE[1]);
        let paddle = |frame: &Capture, columns| {
            white_rows(frame, columns, field.clone()).expect("paddle drawn")
        };
        let start = paddle(&frames[0], left.clone());
        assert_eq!(
            start,
            paddle(&frames[0], right.clone()),
            "paddles start level"
        );
        assert!(
            start.0.abs_diff(SIZE[1] / 2 - 15) <= 3,
            "paddles start centred: {start:?}"
        );
        let ball = |frame: &Capture| white_rows(frame, 100..380, 40..SIZE[1]);
        assert!(ball(&frames[0]).is_some(), "the ball is in play");

        let raised = paddle(&frames[1], left.clone());
        assert!(
            raised.0 + 100 < start.0,
            "W raised the Vertical paddle: {start:?} -> {raised:?}"
        );
        assert_eq!(
            paddle(&frames[1], right.clone()),
            start,
            "the AlternateVertical paddle ignores W"
        );

        assert_eq!(
            ball(&frames[2]),
            None,
            "the scored ball waits hidden for its relaunch"
        );
        // The digits sit in rows 12 to 24, centred on columns 120 and 360, clear of the paddles.
        let score = |frame: &Capture, centre: u32| -> Vec<[u8; 3]> {
            (centre - 20..centre + 20)
                .flat_map(|x| (0..40).map(move |y| (x, y)))
                .map(|(x, y)| frame.rgb(x, y))
                .collect()
        };
        let changed =
            [120, 360].map(|centre| score(&frames[2], centre) != score(&frames[0], centre));
        assert!(
            changed[0] != changed[1],
            "exactly one score changed: {changed:?}"
        );
        assert!(
            [120, 360]
                .iter()
                .all(|&centre| score(&frames[1], centre) == score(&frames[0], centre)),
            "no score changes before the goal"
        );
    }

    /// Breakout's eight bricks per row span columns 105 to 375, with six rows above row 135; the
    /// paddle's red caps sit between rows 215 and 240.
    fn breakout() {
        let frames = play(
            Game::Breakout,
            36,
            vec![key(5, "a", true), key(35, "a", false)],
            &[0, 35],
        );
        let bricks: Vec<u32> = (0..8).map(|brick| 105 + brick * 270 / 8 + 17).collect();
        let mut rows: Vec<[u8; 3]> = Vec::new();
        for y in 36..135 {
            let colours: Vec<[u8; 3]> = bricks.iter().map(|&x| frames[0].rgb(x, y)).collect();
            let colour = colours[0];
            let saturated = colour.iter().max().unwrap() - colour.iter().min().unwrap() > 60;
            if saturated && colours.iter().all(|&other| other == colour) && !rows.contains(&colour)
            {
                let below = frames[0].rgb(bricks[0], y + 3);
                if below == colour {
                    rows.push(colour);
                }
            }
        }
        assert!(
            rows.len() >= 6,
            "six brick rows, each one colour across eight bricks: {rows:?}"
        );

        let paddle = |frame: &Capture| {
            let caps: Vec<u32> = (215..240)
                .flat_map(|y| (0..SIZE[0]).map(move |x| (x, y)))
                .filter(
                    |&(x, y)| matches!(frame.rgb(x, y), [r, g, b] if r > 180 && g < 90 && b < 90),
                )
                .map(|(x, _)| x)
                .collect();
            assert!(!caps.is_empty(), "paddle drawn");
            caps.iter().sum::<u32>() / u32::try_from(caps.len()).unwrap()
        };
        let (start, moved) = (paddle(&frames[0]), paddle(&frames[1]));
        assert!(
            start.abs_diff(SIZE[0] / 2) <= 10,
            "paddle starts centred: {start}"
        );
        assert!(
            moved + 60 < start,
            "A moved the paddle left: {start} -> {moved}"
        );
    }

    /// The centre of the pixels matching `colour`, if any.
    fn centre(frame: &Capture, colour: impl Fn([u8; 3]) -> bool) -> Option<[f64; 2]> {
        let pixels: Vec<(u32, u32)> = (0..SIZE[1])
            .flat_map(|y| (0..SIZE[0]).map(move |x| (x, y)))
            .filter(|&(x, y)| colour(frame.rgb(x, y)))
            .collect();
        let count = f64::from(u32::try_from(pixels.len()).ok()?);
        let sum = pixels.iter().fold([0.0, 0.0], |[sx, sy], &(x, y)| {
            [sx + f64::from(x), sy + f64::from(y)]
        });
        (count > 0.0).then(|| [sum[0] / count, sum[1] / count])
    }

    /// Snake's camera at (19.5, 9.5) shows 22 units over 270 rows, so grid cell `(x, y)` is
    /// centred on column `240 + (x - 19.5) * 270 / 22` and row `135 - (y - 9.5) * 270 / 22`. Both
    /// the white package Square, tinted red for the food and green for the snake, are drawn.
    fn snake() {
        let frames = play(
            Game::Snake,
            61,
            vec![key(30, "w", true), key(32, "w", false)],
            &[0, 29, 60],
        );
        let scale = 270.0 / 22.0;
        let cell = |[x, y]: [f64; 2]| [240.0 + (x - 19.5) * scale, 135.0 - (y - 9.5) * scale];
        let green = |[r, g, b]: [u8; 3]| g > 200 && r < 80 && b < 80;
        let red = |[r, g, b]: [u8; 3]| r > 200 && g < 80 && b < 80;
        let near = |found: Option<[f64; 2]>, cell: [f64; 2], what: &str| {
            let found = found.unwrap_or_else(|| panic!("{what} drawn"));
            assert!(
                (found[0] - cell[0]).abs() < 2.0 && (found[1] - cell[1]).abs() < 2.0,
                "{what} at {found:?}, expected {cell:?}"
            );
        };
        assert!(centre(&frames[0], red).is_some(), "the food is drawn");
        assert!(
            centre(&frames[0], green).is_none(),
            "no segment before the first tick"
        );
        // Four ticks of 1/8 s move the head right from the centre of the 40x20 grid.
        near(
            centre(&frames[1], green),
            cell([24.0, 10.0]),
            "the head moving right",
        );
        // W turns it up; by frame 60 five more ticks have moved it to row 15.
        near(
            centre(&frames[2], green),
            cell([24.0, 15.0]),
            "the head turned up",
        );
        assert_eq!(
            centre(&frames[2], red),
            centre(&frames[0], red),
            "the food stays until eaten"
        );
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    scenarios::run();
}
