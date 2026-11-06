//! `play <snake|pong|breakout>`: plays an original game in a window.
//! `play --player <data directory>`: starts a built Unity player's scene, in a window that draws
//! its 3D objects when `--size`, `--capture` or `--key` is given and headless otherwise.
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use worldrunner_play::{
    Capture, Game, GpuWork, PlayDriver, PlayerWindow, ScheduledKey, key_code, play_player_window,
    prepare, prepare_player, run_player_with,
};
use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

/// The window a player opens in when `--size` is not given.
const WINDOW_SIZE: [u32; 2] = [1920, 1080];

const USAGE: &str = "usage: play <snake|pong|breakout> [--size WxH] [--frames N] [--fixed-dt MS]
            [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--font FILE.ttf] [--seed N]
       play shader-census <player data directory>
       play --player <player data directory> [--scene FILE] [--frames N] [--fixed-dt MS]
            [--size WxH] [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--arg VALUE]...
            [--gpu-work on|off]";

#[derive(Default)]
struct Arguments {
    game: Option<Game>,
    size: Option<[u32; 2]>,
    frames: Option<u64>,
    fixed_dt: Option<Duration>,
    keys: Vec<ScheduledInput>,
    captures: Vec<(u64, PathBuf)>,
    font: Option<PathBuf>,
    seed: Option<i32>,
    player: Option<PathBuf>,
    scene: Option<String>,
    /// Whether a headless player run does its offscreen work on a device.
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
        "--font" => arguments.font = Some(value.into()),
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
fn player(data: &std::path::Path, arguments: &Arguments) -> Result<(), String> {
    if arguments.game.is_some() || arguments.font.is_some() || arguments.seed.is_some() {
        return Err("--player takes no game name, --font or --seed".into());
    }
    let scene = arguments.scene.as_deref().unwrap_or("level0");
    let windowed =
        arguments.size.is_some() || !arguments.keys.is_empty() || !arguments.captures.is_empty();
    // The window opens at `--size`, else at the size the live loop defaults to.
    let screen = windowed.then(|| arguments.size.unwrap_or(WINDOW_SIZE));
    let prepared = prepare_player(data, scene, &arguments.command_line, screen)
        .map_err(|error| error.to_string())?;
    if windowed {
        let window = PlayerWindow {
            title: format!("Disunity: {}", data.display()),
            size: arguments.size.unwrap_or(WINDOW_SIZE),
            fixed_dt: arguments.fixed_dt,
            max_frames: arguments.frames,
            capture_frames: arguments.captures.iter().map(|(frame, _)| *frame).collect(),
            keys: scheduled_keys(&arguments.keys),
        };
        let captured = play_player_window(&prepared, window).map_err(|error| error.to_string())?;
        return write_captures(&captured, &arguments.captures);
    }
    let dt = arguments
        .fixed_dt
        .unwrap_or(Duration::from_nanos(1_000_000_000 / 60));
    run_player_with(
        &prepared,
        arguments.frames.unwrap_or(1),
        dt,
        arguments.gpu_work,
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

fn play(arguments: Arguments) -> Result<(), String> {
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
fn shader_census(directory: &str) -> ExitCode {
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
        && command == "shader-census"
    {
        return shader_census(directory);
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
    fn arg_needs_a_value() {
        assert!(parsed(&["--player", "Game_Data", "--arg"]).is_err());
    }
}
