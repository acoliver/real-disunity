//! `play <snake|pong|breakout>`: plays an original game in a window.
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use worldrunner_play::{Game, PlayDriver, key_code, prepare};
use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

const USAGE: &str = "usage: play <snake|pong|breakout> [--size WxH] [--frames N] [--fixed-dt MS]
            [--key FRAME:KEY:down|up]... [--capture FRAME:FILE.png]... [--font FILE.ttf]";

#[derive(Default)]
struct Arguments {
    game: Option<Game>,
    size: Option<[u32; 2]>,
    frames: Option<u64>,
    fixed_dt: Option<Duration>,
    keys: Vec<ScheduledInput>,
    captures: Vec<(u64, PathBuf)>,
    font: Option<PathBuf>,
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

fn play(arguments: Arguments) -> Result<(), String> {
    let game = arguments.game.ok_or("name a game")?;
    let prepared = prepare(game, arguments.font.as_deref()).map_err(|error| error.to_string())?;
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
