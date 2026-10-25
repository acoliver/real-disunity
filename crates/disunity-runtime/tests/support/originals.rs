use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};
use worldrunner_original_games::{Game, checkout};

pub(super) fn build(root: &Path, dotnet: &std::ffi::OsStr, artifacts: &Path) {
    build_importer(root);
    std::fs::create_dir_all(artifacts.parent().unwrap().join("graphs")).unwrap();
    for case in [
        (Game::Snake, "Snake", "GameScene", 8),
        (Game::Breakout, "Breakout", "SampleScene", 7),
        (Game::Pong, "Pong", "Main", 20),
    ] {
        build_game(root, dotnet, artifacts, case);
    }
}

fn build_importer(root: &Path) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    super::execute(
        Command::new(cargo)
            .current_dir(root)
            .env(
                "CARGO_TARGET_DIR",
                root.join("target/test-artifacts/native"),
            )
            .args([
                "build",
                "--offline",
                "--locked",
                "--jobs",
                "2",
                "-p",
                "worldrunner-compat-content",
                "--example",
                "import",
            ]),
    );
}

fn build_game(
    root: &Path,
    dotnet: &std::ffi::OsStr,
    artifacts: &Path,
    case: (Game, &str, &str, usize),
) {
    let (game, name, scene, count) = case;
    let source = checkout(game).expect("pinned original checkout");
    let before = sources(&source.join("Assets"));
    assert_eq!(before.len(), count, "{name} runtime source count");
    super::build(
        root,
        dotnet,
        artifacts,
        &format!("Compatibility.Tests/OriginalGames/{name}"),
        Some(&source),
    );
    let manifest = artifacts.join(format!("obj/{name}/debug/original-sources.txt"));
    let inputs: BTreeSet<PathBuf> = std::fs::read_to_string(manifest)
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .map(PathBuf::from)
        .filter(|path| path.starts_with(&source))
        .collect();
    assert_eq!(
        inputs,
        before.keys().cloned().collect(),
        "{name} compiler inputs"
    );
    assert_eq!(
        sources(&source.join("Assets")),
        before,
        "source changed during build"
    );
    let content = root.join("crates/compat-content");
    let graph = super::execute(
        Command::new(root.join("target/test-artifacts/native/debug/examples/import"))
            .arg(&source)
            .arg(format!("Assets/Scenes/{scene}.unity"))
            .arg(content.join(format!("profiles/{}-source.json", game.name())))
            .arg(content.join(format!("overlays/{}.json", game.name()))),
    );
    let value: serde_json::Value = serde_json::from_str(&graph).unwrap();
    assert!(
        value["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
    );
    std::fs::write(
        artifacts
            .parent()
            .unwrap()
            .join("graphs")
            .join(format!("{}-ready.json", game.name())),
        graph,
    )
    .unwrap();
}

fn sources(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !matches!(
                path.file_name().unwrap().to_str(),
                Some("Tests" | "Editor" | "obj" | "bin")
            ) {
                result.extend(sources(&path));
            }
        } else if path.extension().is_some_and(|ext| ext == "cs") {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}
