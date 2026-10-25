use worldrunner_original_games::{Game, checkout_in, workspace_root};

fn run() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let root = if args.first().is_some_and(|arg| arg == "--workspace") {
        if args.len() < 2 {
            return Err("--workspace requires a path".into());
        }
        let root = std::path::PathBuf::from(args.remove(1));
        args.remove(0);
        root
    } else {
        workspace_root().map_err(|error| error.to_string())?
    };
    let games =
        match args.as_slice() {
            [command, name] if command == "path" => {
                vec![name.parse::<Game>().map_err(|error| error.to_string())?]
            }
            [command] if command == "fetch-all" => Game::ALL.to_vec(),
            _ => return Err(
                "usage: original-games [--workspace <root>] path <snake|breakout|pong> | fetch-all"
                    .into(),
            ),
        };
    for game in games {
        println!(
            "{}",
            checkout_in(&root, game)
                .map_err(|error| error.to_string())?
                .display()
        );
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
