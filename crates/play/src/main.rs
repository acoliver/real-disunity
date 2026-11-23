//! `play <snake|pong|breakout>`: plays an original game in a window.
//! `play --player <data directory>`: starts a built Unity player's scene, in a window that draws
//! its 3D objects when `--size`, `--capture` or `--key` is given, in a windowless host that writes
//! the frames `--screen` names, and with no viewer otherwise.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use worldrunner_play::Game;
use worldrunner_play::PlayerLaunch;
use worldrunner_play::PlayerWindow;
use worldrunner_play::ScheduledKey;
use worldrunner_play::StandInLook;
use worldrunner_play::SuppliedGlobal;
use worldrunner_play::TargetRead;
use worldrunner_play::key_code;
use worldrunner_play::play_game_window;
use worldrunner_play::play_player_window;
use worldrunner_play::play_player_windowless;
use worldrunner_play::prepare;
use worldrunner_play::prepare_player;
use worldrunner_play::run_player;

/// The window a player opens in when `--size` is not given.
const WINDOW_SIZE: [u32; 2] = [1920, 1080];

const USAGE: &str = "usage: play <snake|pong|breakout> [--size WxH] [--frames N] [--fixed-dt MS]
            [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--font FILE.ttf] [--seed N]
       play shader-census <player data directory> [texture property]...
       play compute-census <player data directory>
       play --player <player data directory> [--scene FILE] [--frames N] [--fixed-dt MS]
            [--size WxH] [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--arg VALUE]...
            [--screen FRAME:FILE.png]... [--stand-ins declared|magenta]
            [--report-each-frame] [--global NAME=v0,v1,v2,v3]...
            [--read-target FRAME:RECORD:FILE.png]... [--plugins DIRECTORY]...";

#[derive(Default)]
struct Arguments {
    game: Option<Game>,
    size: Option<[u32; 2]>,
    frames: Option<u64>,
    fixed_dt: Option<Duration>,
    keys: Vec<ScheduledKey>,
    captures: Vec<(u64, PathBuf)>,
    /// The frames of a windowless player run whose image is written, and the files.
    screens: Vec<(u64, PathBuf)>,
    /// The reads of a record's target that `--read-target` asks for, in the order given.
    reads: Vec<TargetRead>,
    font: Option<PathBuf>,
    /// Extra native plugin directories for a player, in the order given.
    plugins: Vec<PathBuf>,
    seed: Option<i32>,
    player: Option<PathBuf>,
    scene: Option<String>,
    /// The globals `--global` supplies for the run, in the order given.
    globals: Vec<SuppliedGlobal>,
    /// How the stand-in textures look: `--stand-ins`.
    stand_ins: StandInLook,
    /// Whether `--report-each-frame` was given.
    report_each_frame: bool,
    /// The player's command line after its executable path, in the order given.
    command_line: Vec<String>,
}

fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{what} `{text}` is not a number"))
}

/// `FRAME:KEY:down|up`, with KEY a key name such as `w`, `space` or `arrowup`.
fn key(text: &str) -> Result<ScheduledKey, String> {
    let [frame, key, state] = text.split(':').collect::<Vec<_>>()[..] else {
        return Err(format!("--key `{text}` is not FRAME:KEY:down|up"));
    };
    let Some(code) = key_code(key) else {
        return Err(format!("--key `{text}`: the engine has no key `{key}`"));
    };
    let pressed = match state {
        "down" => true,
        "up" => false,
        _ => return Err(format!("--key `{text}`: the state is down or up")),
    };
    Ok(ScheduledKey {
        frame: number(frame, "--key frame")?,
        code,
        pressed,
    })
}

fn option(arguments: &mut Arguments, name: &str, value: &str) -> Result<(), String> {
    match name {
        "--size" => {
            let (width, height) = value
                .split_once('x')
                .ok_or_else(|| format!("--size `{value}` is not WxH"))?;
            arguments.size = Some([
                number(width, "--size width")?,
                number(height, "--size height")?,
            ]);
        }
        "--frames" => arguments.frames = Some(number(value, "--frames")?),
        "--fixed-dt" => {
            arguments.fixed_dt = Some(Duration::from_secs_f64(
                number::<f64>(value, "--fixed-dt")? / 1000.,
            ));
        }
        "--key" => arguments.keys.push(key(value)?),
        "--capture" => {
            let (frame, file) = value
                .split_once(':')
                .ok_or_else(|| format!("--capture `{value}` is not FRAME:FILE"))?;
            arguments
                .captures
                .push((number(frame, "--capture frame")?, file.into()));
        }
        "--screen" => {
            let (frame, file) = value
                .split_once(':')
                .ok_or_else(|| format!("--screen `{value}` is not FRAME:FILE"))?;
            arguments
                .screens
                .push((number(frame, "--screen frame")?, file.into()));
        }
        "--read-target" => {
            let read = TargetRead::parse(value)
                .and_then(|read| read.check_new(&arguments.reads).map(|()| read))
                .map_err(|reason| format!("--read-target `{value}`: {reason}"))?;
            arguments.reads.push(read);
        }
        "--font" => arguments.font = Some(value.into()),
        "--plugins" => arguments.plugins.push(value.into()),
        "--seed" => arguments.seed = Some(number(value, "--seed")?),
        "--player" => arguments.player = Some(value.into()),
        "--scene" => arguments.scene = Some(value.to_owned()),
        "--stand-ins" => {
            arguments.stand_ins = StandInLook::from_name(value)
                .ok_or_else(|| format!("--stand-ins `{value}` is declared or magenta"))?;
        }
        "--global" => {
            let global = SuppliedGlobal::parse(value)
                .map_err(|reason| format!("--global `{value}`: {reason}"))?;
            if arguments
                .globals
                .iter()
                .any(|held| held.name() == global.name())
            {
                return Err(format!(
                    "--global `{value}`: {} is given twice",
                    global.name()
                ));
            }
            arguments.globals.push(global);
        }
        "--arg" => arguments.command_line.push(value.to_owned()),
        _ => return Err(format!("unknown option {name}")),
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut arguments = Arguments::default();
    while let Some(arg) = args.next() {
        if arg == "--report-each-frame" {
            arguments.report_each_frame = true;
        } else if arg.starts_with("--") {
            let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
            option(&mut arguments, &arg, &value)?;
        } else if arguments.game.is_none() {
            arguments.game = Some(arg.parse().map_err(|error| format!("{error}"))?);
        } else {
            return Err(format!("unexpected argument {arg}"));
        }
    }
    Ok(arguments)
}

/// Starts a player's scene, `level0` unless `--scene` names another. With `--size`,
/// `--capture` or `--key` it plays in a window like an original game; with `--screen` it plays
/// the same driver in a windowless host, at `--size` (else the window's default) with no display,
/// for `--frames` frames (one by default) of `--fixed-dt` (60 Hz by default), and writes the
/// frame each `--screen` names: the image after that game frame, the screen the player's
/// offscreen work drew into with the game's UI over it. Otherwise it runs with no viewer for
/// `--frames` frames of the same step, which is also how `--read-target` writes its reads, at
/// `--size` (else the window's default). Each `--arg` is one argument of the player's command line,
/// after its executable path.
fn player(data: &std::path::Path, arguments: &Arguments) -> Result<(), String> {
    if arguments.game.is_some() || arguments.font.is_some() || arguments.seed.is_some() {
        return Err("--player takes no game name, --font or --seed".into());
    }
    let scene = arguments.scene.as_deref().unwrap_or("level0");
    let screens = !arguments.screens.is_empty();
    let reading = !arguments.reads.is_empty();
    let windowed = !screens
        && !reading
        && (arguments.size.is_some()
            || !arguments.keys.is_empty()
            || !arguments.captures.is_empty());
    // The window opens at `--size`, else at `WINDOW_SIZE`; a windowless run that writes the
    // screen has the same size, which the player reads as its own screen.
    let size = arguments.size.unwrap_or(WINDOW_SIZE);
    let launch = PlayerLaunch {
        arguments: &arguments.command_line,
        screen: (windowed || screens || reading).then_some(size),
        plugins: &arguments.plugins,
        globals: &arguments.globals,
        stand_ins: arguments.stand_ins,
        report_each_frame: arguments.report_each_frame,
        read_targets: &arguments.reads,
    };
    let prepared = prepare_player(data, scene, launch).map_err(|error| error.to_string())?;
    let dt = arguments
        .fixed_dt
        .unwrap_or(Duration::from_nanos(1_000_000_000 / 60));
    let window = |frames: Option<u64>, requested: &[(u64, PathBuf)], fixed_dt| PlayerWindow {
        title: format!("Disunity: {}", data.display()),
        size,
        fixed_dt,
        max_frames: frames,
        capture_frames: BTreeSet::new(),
        capture_files: requested.to_vec(),
        keys: arguments.keys.clone(),
    };
    if screens {
        let window = window(
            Some(arguments.frames.unwrap_or(1)),
            &arguments.screens,
            Some(dt),
        );
        return play_player_windowless(&prepared, &window)
            .map(drop)
            .map_err(|error| error.to_string());
    }
    if windowed {
        let window = window(arguments.frames, &arguments.captures, arguments.fixed_dt);
        return play_player_window(&prepared, &window)
            .map(drop)
            .map_err(|error| error.to_string());
    }
    run_player(&prepared, arguments.frames.unwrap_or(1), dt).map_err(|error| error.to_string())
}

/// Plays `prepared` in a window of `--size` (else `size`) and writes the requested captures.
fn play_game(
    prepared: worldrunner_play::Prepared,
    title: String,
    size: [u32; 2],
    arguments: Arguments,
) -> Result<(), String> {
    let window = PlayerWindow {
        title,
        size: arguments.size.unwrap_or(size),
        fixed_dt: arguments.fixed_dt,
        max_frames: arguments.frames,
        capture_frames: BTreeSet::new(),
        capture_files: arguments.captures,
        keys: arguments.keys,
    };
    play_game_window(prepared, &window)
        .map(drop)
        .map_err(|error| error.to_string())
}

/// A screen image comes from a windowless player run.
fn check_screens(arguments: &Arguments) -> Result<(), String> {
    if arguments.screens.is_empty() {
        return Ok(());
    }
    if arguments.player.is_none() {
        return Err("--screen applies only to --player".into());
    }
    if !arguments.keys.is_empty() || !arguments.captures.is_empty() {
        return Err(
            "--screen writes from a windowless run, which takes no --key or --capture".into(),
        );
    }
    Ok(())
}

/// Native plugin directories apply to a player's imports; an original game has none.
fn check_plugins(arguments: &Arguments) -> Result<(), String> {
    if arguments.player.is_none() && !arguments.plugins.is_empty() {
        return Err("--plugins applies only to --player".into());
    }
    Ok(())
}

/// `--global` values start the offscreen work of a player's frames, in a window and headless
/// alike, so it needs a player.
fn check_globals(arguments: &Arguments) -> Result<(), String> {
    if !arguments.globals.is_empty() && arguments.player.is_none() {
        return Err("--global applies only to --player".into());
    }
    Ok(())
}

/// A read of a record's target comes from a headless player run that plays its frame.
fn check_reads(arguments: &Arguments) -> Result<(), String> {
    if arguments.reads.is_empty() {
        return Ok(());
    }
    if arguments.player.is_none() {
        return Err("--read-target applies only to --player".into());
    }
    if !arguments.keys.is_empty() || !arguments.captures.is_empty() || !arguments.screens.is_empty()
    {
        return Err(
            "--read-target reads from a headless run, which takes no --screen, --key or --capture"
                .into(),
        );
    }
    TargetRead::check_frames(&arguments.reads, arguments.frames.unwrap_or(1))
}

fn play(arguments: Arguments) -> Result<(), String> {
    check_plugins(&arguments)?;
    check_screens(&arguments)?;
    check_reads(&arguments)?;
    check_globals(&arguments)?;
    if let Some(data) = arguments.player.clone() {
        return player(&data, &arguments);
    }
    let game = arguments.game.ok_or("name a game")?;
    let prepared = prepare(game, arguments.font.as_deref(), arguments.seed)
        .map_err(|error| error.to_string())?;
    play_game(
        prepared,
        format!("Disunity: {}", game.name()),
        [960, 540],
        arguments,
    )
}

/// `play shader-census <player data directory>`: loads every shader of the player through the
/// offscreen adapter's loader and prints counts.
fn shader_census(directory: &str, properties: &[String]) -> ExitCode {
    match worldrunner_play::offscreen::shader_census(std::path::Path::new(directory)) {
        Ok(census) => {
            println!(
                "{} shader objects, {} loaded with {} passes, failed by error: {:?}",
                census.shaders, census.loaded, census.passes, census.failed
            );
            println!(
                "{} loaded shaders have no pass with a LightMode tag; passes by LightMode: {:?}",
                census.shaders_without_light_mode, census.light_modes
            );
            for property in properties {
                println!(
                    "{property}: shaders by declared default {:?}",
                    census.declared_defaults.get(property)
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// `play compute-census <player data directory>`: builds every compute kernel variant of the
/// player on the headless player's device and prints counts.
fn compute_census(directory: &str) -> ExitCode {
    match worldrunner_play::compute_census(std::path::Path::new(directory)) {
        Ok((device, census)) => {
            println!(
                "{} compute shader objects, {} kernel variants, {} built as compute programs on {device}, \
                 left out by cause: {:?}",
                census.shaders, census.variants, census.programs, census.failed
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [command, directory] = args.as_slice()
        && command == "compute-census"
    {
        return compute_census(directory);
    }
    if let [command, directory, properties @ ..] = args.as_slice()
        && command == "shader-census"
    {
        return shader_census(directory, properties);
    }
    match parse(std::env::args().skip(1)).and_then(play) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn parsed(args: &[&str]) -> Result<super::Arguments, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn repeated_arg_options_keep_their_order_and_take_dashed_values() {
        let arguments = parsed(&[
            "--player",
            "Game_Data",
            "--arg",
            "-userdata",
            "--arg",
            "/saves",
            "--arg",
            "-headless",
            "--arg",
            "--flag",
            "--frames",
            "3",
        ])
        .expect("parses");
        assert_eq!(
            arguments.command_line,
            ["-userdata", "/saves", "-headless", "--flag"]
        );
        assert_eq!(arguments.frames, Some(3));
    }

    #[test]
    fn stand_ins_look_declared_unless_the_run_says_magenta() {
        use worldrunner_play::StandInLook;
        let look = |extra: &[&str]| {
            let mut args = vec!["--player", "Game_Data"];
            args.extend_from_slice(extra);
            parsed(&args).expect("parses").stand_ins
        };
        assert_eq!(look(&[]), StandInLook::Declared);
        assert_eq!(look(&["--stand-ins", "declared"]), StandInLook::Declared);
        assert_eq!(look(&["--stand-ins", "magenta"]), StandInLook::Magenta);
        assert!(parsed(&["--player", "Game_Data", "--stand-ins", "pink"]).is_err());
    }

    #[test]
    fn the_gpu_storage_budget_has_no_option_because_the_adapter_sets_it() {
        for value in ["1536", "0", "lots"] {
            let error = parsed(&["--player", "Game_Data", "--gpu-storage-mb", value])
                .map(|_| ())
                .unwrap_err();
            assert_eq!(error, "unknown option --gpu-storage-mb", "{value}");
        }
        assert!(!super::USAGE.contains("gpu-storage"));
    }

    #[test]
    fn the_report_after_each_frame_is_a_switch_that_takes_no_value() {
        let on = parsed(&[
            "--player",
            "Game_Data",
            "--report-each-frame",
            "--frames",
            "2",
        ])
        .expect("parses");
        assert!(on.report_each_frame);
        assert_eq!(on.frames, Some(2));
        assert!(
            !parsed(&["--player", "Game_Data"])
                .expect("parses")
                .report_each_frame
        );
    }

    #[test]
    fn global_options_supply_named_values_in_the_order_given() {
        let arguments = parsed(&[
            "--player",
            "Game_Data",
            "--global",
            "unity_LightData=1,1,1,1",
            "--global",
            "_Other=0.5,2",
        ])
        .expect("parses");
        let names: Vec<_> = arguments
            .globals
            .iter()
            .map(worldrunner_play::SuppliedGlobal::name)
            .collect();
        assert_eq!(names, ["unity_LightData", "_Other"]);
        let bits = |index: usize| arguments.globals[index].value().map(f32::to_bits);
        assert_eq!(bits(0), [1.0_f32; 4].map(f32::to_bits));
        assert_eq!(bits(1), [0.5_f32, 2.0, 0.0, 0.0].map(f32::to_bits));
        let none = parsed(&["--player", "Game_Data"]).expect("parses");
        assert!(none.globals.is_empty());
    }

    #[test]
    fn a_malformed_global_option_is_refused_naming_the_option_and_the_value() {
        for value in [
            "unity_LightData",
            "unity_LightData=",
            "unity_LightData=1,2,3,4,5",
            "unity_LightData=1,x",
            "9name=1",
            "a b=1",
            "a=nan",
        ] {
            let error = parsed(&["--player", "Game_Data", "--global", value])
                .err()
                .unwrap_or_else(|| panic!("`{value}` parsed"));
            assert!(error.starts_with(&format!("--global `{value}`")), "{error}");
        }
        let twice = parsed(&["--global", "a=1", "--global", "a=2"])
            .err()
            .unwrap();
        assert!(
            twice.contains("`a=2`") && twice.contains("given twice"),
            "{twice}"
        );
        assert!(parsed(&["--player", "Game_Data", "--global"]).is_err());
    }

    #[test]
    fn a_global_reaches_a_player_that_does_its_offscreen_work_headless_or_in_a_window() {
        for extra in [
            &[][..],
            &["--size", "64x64"][..],
            &["--key", "1:w:down"][..],
            &["--capture", "1:out.png"][..],
        ] {
            let mut args = vec!["--player", "Game_Data", "--global", "a=1"];
            args.extend_from_slice(extra);
            let arguments = parsed(&args).expect("parses");
            assert_eq!(super::check_globals(&arguments), Ok(()), "{extra:?}");
        }
    }

    #[test]
    fn a_windowed_player_with_a_global_is_not_refused_for_it() {
        let arguments = parsed(&[
            "--player",
            "Game_Data",
            "--size",
            "64x64",
            "--global",
            "a=1",
        ])
        .expect("parses");
        // The run goes on to read the player's data, which does not exist here; the refusal of
        // `--global` is not what stops it.
        let error = super::player(std::path::Path::new("Game_Data"), &arguments).unwrap_err();
        assert!(!error.contains("--global"), "{error}");
    }

    #[test]
    fn a_global_is_refused_for_a_run_that_is_not_a_player() {
        for args in [
            &["snake", "--global", "a=1"][..],
            &["--global", "a=1"][..],
            &["pong", "--size", "64x64", "--global", "a=1"][..],
        ] {
            let arguments = parsed(args).expect("parses");
            let error = super::check_globals(&arguments).unwrap_err();
            assert_eq!(error, "--global applies only to --player", "{args:?}");
            let error = super::play(parsed(args).expect("parses")).unwrap_err();
            assert_eq!(error, "--global applies only to --player", "{args:?}");
        }
        let plain = parsed(&["snake", "--size", "64x64"]).expect("parses");
        assert_eq!(super::check_globals(&plain), Ok(()));
    }

    #[test]
    fn repeated_plugins_options_keep_their_order() {
        let arguments = parsed(&[
            "--player",
            "Game_Data",
            "--plugins",
            "/a/Plugins",
            "--plugins",
            "b",
        ])
        .expect("parses");
        assert_eq!(
            arguments.plugins,
            [
                std::path::PathBuf::from("/a/Plugins"),
                std::path::PathBuf::from("b")
            ]
        );
    }

    #[test]
    fn plugins_needs_a_value() {
        assert!(parsed(&["--player", "Game_Data", "--plugins"]).is_err());
    }

    #[test]
    fn plugins_belong_to_a_player() {
        let game = parsed(&["snake", "--plugins", "/a"]).expect("parses");
        assert!(super::check_plugins(&game).is_err());
        let player = parsed(&["--player", "Game_Data", "--plugins", "/a"]).expect("parses");
        assert!(super::check_plugins(&player).is_ok());
        assert!(super::check_plugins(&parsed(&["snake"]).expect("parses")).is_ok());
    }

    #[test]
    fn repeated_screen_options_keep_their_frames_and_files_in_order() {
        let arguments = parsed(&[
            "--player",
            "Game_Data",
            "--screen",
            "3:a.png",
            "--screen",
            "10:dir/b.png",
        ])
        .expect("parses");
        assert_eq!(
            arguments.screens,
            [
                (3, std::path::PathBuf::from("a.png")),
                (10, std::path::PathBuf::from("dir/b.png"))
            ]
        );
        assert!(parsed(&["--player", "Game_Data", "--screen", "a.png"]).is_err());
        assert!(parsed(&["--player", "Game_Data", "--screen", "x:a.png"]).is_err());
    }

    fn checked(args: &[&str]) -> Result<(), String> {
        super::check_screens(&parsed(args).expect("parses"))
    }

    #[test]
    fn a_screen_belongs_to_a_windowless_player_run() {
        let screen = ["--player", "Game_Data", "--screen", "0:a.png"];
        assert!(checked(&screen).is_ok());
        assert!(checked(&[&screen[..], &["--size", "64x32", "--frames", "2"]].concat()).is_ok());
        assert!(
            checked(&["snake", "--screen", "0:a.png"]).is_err(),
            "not a game's"
        );
        for window in [["--capture", "0:w.png"], ["--key", "0:w:down"]] {
            assert!(
                checked(&[&screen[..], &window[..]].concat()).is_err(),
                "{window:?}"
            );
        }
        assert!(checked(&["--player", "Game_Data", "--capture", "0:w.png"]).is_ok());
    }

    #[test]
    fn arg_needs_a_value() {
        assert!(parsed(&["--player", "Game_Data", "--arg"]).is_err());
    }
}

#[cfg(test)]
#[path = "main_read_target_tests.rs"]
mod read_target_tests;
