//! `play <snake|pong|breakout>`: plays an original game in a window.
//! `play --player <data directory>`: starts a built Unity player's scene, in a window that draws
//! its 3D objects when `--size`, `--capture` or `--key` is given and headless otherwise.
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use worldrunner_play::{
    Capture, Game, GpuWork, PlayDriver, PlayerLaunch, PlayerWindow, ScheduledKey, ScreenOutput,
    key_code, play_player_window, prepare, prepare_player, run_player_screens,
};
use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

/// The window a player opens in when `--size` is not given.
const WINDOW_SIZE: [u32; 2] = [1920, 1080];

const USAGE: &str = "usage: play <snake|pong|breakout> [--size WxH] [--frames N] [--fixed-dt MS]
            [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--font FILE.ttf] [--seed N]
       play shader-census <player data directory> [texture property]...
       play compute-census <player data directory>
       play --player <player data directory> [--scene FILE] [--frames N] [--fixed-dt MS]
            [--size WxH] [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--arg VALUE]...
            [--screen FRAME:FILE.png]... [--gpu-work on|off]
            [--plugins DIRECTORY]...";

#[derive(Default)]
struct Arguments {
    game: Option<Game>,
    size: Option<[u32; 2]>,
    frames: Option<u64>,
    fixed_dt: Option<Duration>,
    keys: Vec<ScheduledInput>,
    captures: Vec<(u64, PathBuf)>,
    /// The frames of a headless player run whose screen image is written, and the files.
    screens: Vec<(u64, PathBuf)>,
    font: Option<PathBuf>,
    /// Extra native plugin directories for a player, in the order given.
    plugins: Vec<PathBuf>,
    seed: Option<i32>,
    player: Option<PathBuf>,
    scene: Option<String>,
    /// Whether a player run, headless or in a window, does its offscreen work on a device.
    gpu_work: GpuWork,
    /// The player's command line after its executable path, in the order given.
    command_line: Vec<String>,
}

fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{what} `{text}` is not a number"))
}

/// `FRAME:KEY:down|up`, with KEY a key name such as `w`, `space` or `arrowup`.
fn key(text: &str) -> Result<ScheduledInput, String> {
    let [frame, key, state] = text.split(':').collect::<Vec<_>>()[..] else {
        return Err(format!("--key `{text}` is not FRAME:KEY:down|up"));
    };
    if key_code(key).is_none() {
        return Err(format!("--key `{text}`: the engine has no key `{key}`"));
    }
    let pressed = match state {
        "down" => true,
        "up" => false,
        _ => return Err(format!("--key `{text}`: the state is down or up")),
    };
    let event = LiveInputEvent::Key {
        key: key.to_owned(),
        pressed,
        repeat: false,
    };
    Ok(ScheduledInput {
        frame: number(frame, "--key frame")?,
        event,
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
        "--font" => arguments.font = Some(value.into()),
        "--plugins" => arguments.plugins.push(value.into()),
        "--seed" => arguments.seed = Some(number(value, "--seed")?),
        "--player" => arguments.player = Some(value.into()),
        "--scene" => arguments.scene = Some(value.to_owned()),
        "--gpu-work" => {
            arguments.gpu_work = match value {
                "on" => GpuWork::Auto,
                "off" => GpuWork::Off,
                _ => return Err(format!("--gpu-work `{value}` is on or off")),
            };
        }
        "--arg" => arguments.command_line.push(value.to_owned()),
        _ => return Err(format!("unknown option {name}")),
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut arguments = Arguments::default();
    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
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
/// `--capture` or `--key` it plays in a window like an original game; otherwise it runs
/// headless for `--frames` frames (one by default) of `--fixed-dt` (60 Hz by default).
/// Each `--arg` is one argument of the player's command line, after its executable path.
/// With `--screen` the run is headless, at `--size` (else the window's default), and writes
/// the screen the player's offscreen work drew into after each frame named.
fn player(data: &std::path::Path, arguments: &Arguments) -> Result<(), String> {
    if arguments.game.is_some() || arguments.font.is_some() || arguments.seed.is_some() {
        return Err("--player takes no game name, --font or --seed".into());
    }
    let scene = arguments.scene.as_deref().unwrap_or("level0");
    let screens = !arguments.screens.is_empty();
    let windowed = !screens
        && (arguments.size.is_some()
            || !arguments.keys.is_empty()
            || !arguments.captures.is_empty());
    // The window opens at `--size`, else at the size the live loop defaults to; a headless run
    // that writes the screen has the same size, which the player reads as its own screen.
    let size = arguments.size.unwrap_or(WINDOW_SIZE);
    let launch = PlayerLaunch {
        arguments: &arguments.command_line,
        screen: (windowed || screens).then_some(size),
        plugins: &arguments.plugins,
    };
    let prepared = prepare_player(data, scene, launch).map_err(|error| error.to_string())?;
    if windowed {
        let window = PlayerWindow {
            title: format!("Disunity: {}", data.display()),
            size,
            fixed_dt: arguments.fixed_dt,
            max_frames: arguments.frames,
            capture_frames: arguments.captures.iter().map(|(frame, _)| *frame).collect(),
            keys: scheduled_keys(&arguments.keys),
            gpu_work: arguments.gpu_work,
        };
        let captured = play_player_window(&prepared, window).map_err(|error| error.to_string())?;
        return write_captures(&captured, &arguments.captures);
    }
    let dt = arguments
        .fixed_dt
        .unwrap_or(Duration::from_nanos(1_000_000_000 / 60));
    let output = screens.then(|| ScreenOutput {
        size,
        files: arguments.screens.clone(),
    });
    run_player_screens(
        &prepared,
        arguments.frames.unwrap_or(1),
        dt,
        arguments.gpu_work,
        output.as_ref(),
    )
    .map_err(|error| error.to_string())
}

/// Plays `driver` in a window of `--size` (else `size`) and writes the requested captures.
fn live(
    mut driver: PlayDriver,
    title: String,
    size: [u32; 2],
    arguments: Arguments,
) -> Result<(), String> {
    let mut options = LiveOptions::new(title, arguments.size.unwrap_or(size));
    options.fixed_dt = arguments.fixed_dt;
    options.max_frames = arguments.frames;
    options.scheduled_input = arguments.keys;
    run_live(&mut driver, options).map_err(|error| error.to_string())?;
    write_captures(driver.captures(), &arguments.captures)
}

/// The keys the command line schedules, as the game's key codes.
fn scheduled_keys(keys: &[ScheduledInput]) -> Vec<ScheduledKey> {
    keys.iter()
        .filter_map(|input| match &input.event {
            LiveInputEvent::Key { key, pressed, .. } => Some(ScheduledKey {
                frame: input.frame,
                code: key_code(key)?,
                pressed: *pressed,
            }),
            _ => None,
        })
        .collect()
}

/// Writes each captured frame to the file requested for it, and fails when a requested frame
/// was never presented.
fn write_captures(captured: &[Capture], requested: &[(u64, PathBuf)]) -> Result<(), String> {
    for capture in captured {
        let (_, file) = requested
            .iter()
            .find(|(frame, _)| *frame == capture.frame)
            .expect("requested capture");
        capture.write_png(file).map_err(|error| error.to_string())?;
        println!("frame {} captured to {}", capture.frame, file.display());
    }
    let missing = requested.len() - captured.len();
    if missing > 0 {
        return Err(format!("{missing} requested frames were not presented"));
    }
    Ok(())
}

/// A screen image comes from a headless player run with the offscreen work on a device.
fn check_screens(arguments: &Arguments) -> Result<(), String> {
    if arguments.screens.is_empty() {
        return Ok(());
    }
    if arguments.player.is_none() {
        return Err("--screen applies only to --player".into());
    }
    if !arguments.keys.is_empty() || !arguments.captures.is_empty() {
        return Err(
            "--screen writes from a headless run, which takes no --key or --capture".into(),
        );
    }
    if arguments.gpu_work == GpuWork::Off {
        return Err("--screen needs --gpu-work on".into());
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

fn play(arguments: Arguments) -> Result<(), String> {
    check_plugins(&arguments)?;
    check_screens(&arguments)?;
    if let Some(data) = arguments.player.clone() {
        return player(&data, &arguments);
    }
    let game = arguments.game.ok_or("name a game")?;
    let prepared = prepare(game, arguments.font.as_deref(), arguments.seed)
        .map_err(|error| error.to_string())?;
    let driver = PlayDriver::new(prepared, arguments.captures.iter().map(|(frame, _)| *frame))
        .map_err(|error| error.to_string())?;
    live(
        driver,
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
    fn gpu_work_is_on_unless_the_run_says_off() {
        use worldrunner_play::GpuWork;
        assert_eq!(
            parsed(&["--player", "Game_Data"]).expect("parses").gpu_work,
            GpuWork::Auto
        );
        assert_eq!(
            parsed(&["--player", "Game_Data", "--gpu-work", "off"])
                .expect("parses")
                .gpu_work,
            GpuWork::Off
        );
        assert_eq!(
            parsed(&["--player", "Game_Data", "--gpu-work", "on"])
                .expect("parses")
                .gpu_work,
            GpuWork::Auto
        );
        assert!(parsed(&["--player", "Game_Data", "--gpu-work", "maybe"]).is_err());
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
    fn a_screen_belongs_to_a_headless_player_run_with_gpu_work_on() {
        let screen = ["--player", "Game_Data", "--screen", "0:a.png"];
        assert!(checked(&screen).is_ok());
        assert!(checked(&[&screen[..], &["--size", "64x32", "--frames", "2"]].concat()).is_ok());
        assert!(
            checked(&["snake", "--screen", "0:a.png"]).is_err(),
            "not a game's"
        );
        for window in [
            ["--capture", "0:w.png"],
            ["--key", "0:w:down"],
            ["--gpu-work", "off"],
        ] {
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
