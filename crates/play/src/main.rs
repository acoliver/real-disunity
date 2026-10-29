//! `play <snake|pong|breakout>`: plays an original game in a window.
//! `play --player <data directory>`: starts a built Unity player's scene without a window.
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use worldrunner_play::{Game, PlayDriver, key_code, prepare, prepare_player, run_player};
use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

const USAGE: &str = "usage: play <snake|pong|breakout> [--size WxH] [--frames N] [--fixed-dt MS]
            [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--font FILE.ttf] [--seed N]
       play --player <player data directory> [--scene FILE] [--frames N] [--fixed-dt MS]
            [--arg VALUE]...";

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

/// Starts a player's scene, `level0` unless `--scene` names another, and runs it headless
/// for `--frames` frames (one by default) of `--fixed-dt` (60 Hz by default). Each `--arg`
/// is one argument of the player's command line, after its executable path.
fn player(data: &std::path::Path, arguments: &Arguments) -> Result<(), String> {
    let windowed = arguments.game.is_some()
        || arguments.size.is_some()
        || !arguments.keys.is_empty()
        || !arguments.captures.is_empty()
        || arguments.font.is_some()
        || arguments.seed.is_some();
    if windowed {
        return Err(
            "--player runs without a window and takes only --scene, --frames, --fixed-dt and --arg"
                .into(),
        );
    }
    let scene = arguments.scene.as_deref().unwrap_or("level0");
    let prepared =
        prepare_player(data, scene, &arguments.command_line).map_err(|error| error.to_string())?;
    let dt = arguments
        .fixed_dt
        .unwrap_or(Duration::from_nanos(1_000_000_000 / 60));
    run_player(&prepared, arguments.frames.unwrap_or(1), dt).map_err(|error| error.to_string())
}

fn play(arguments: Arguments) -> Result<(), String> {
    if let Some(data) = &arguments.player {
        return player(data, &arguments);
    }
    let game = arguments.game.ok_or("name a game")?;
    let prepared = prepare(game, arguments.font.as_deref(), arguments.seed)
        .map_err(|error| error.to_string())?;
    let mut driver = PlayDriver::new(prepared, arguments.captures.iter().map(|(frame, _)| *frame))
        .map_err(|error| error.to_string())?;
    let mut options = LiveOptions::new(
        format!("Disunity: {}", game.name()),
        arguments.size.unwrap_or([960, 540]),
    );
    options.fixed_dt = arguments.fixed_dt;
    options.max_frames = arguments.frames;
    options.scheduled_input = arguments.keys;
    run_live(&mut driver, options).map_err(|error| error.to_string())?;
    for capture in driver.captures() {
        let (_, file) = arguments
            .captures
            .iter()
            .find(|(frame, _)| *frame == capture.frame)
            .expect("requested capture");
        capture.write_png(file).map_err(|error| error.to_string())?;
        println!("frame {} captured to {}", capture.frame, file.display());
    }
    let missing = arguments.captures.len() - driver.captures().len();
    if missing > 0 {
        return Err(format!("{missing} requested frames were not presented"));
    }
    Ok(())
}

fn main() -> ExitCode {
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
    fn arg_needs_a_value() {
        assert!(parsed(&["--player", "Game_Data", "--arg"]).is_err());
    }
}
