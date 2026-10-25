use std::{fs, path::Path, process::Command};
use worldrunner_original_games::{Game, checkout_in, workspace_root};

fn assert_original(root: &Path, game: Game) {
    let scenes = root.join("Assets/Scenes");
    assert!(
        fs::read_dir(&scenes).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "unity")
        }),
        "{} has no original scenes",
        root.display()
    );
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(String::from_utf8(head.stdout).unwrap().trim(), game.rev());
}

#[test]
fn pinned_originals_fetch_with_concurrent_readers_and_repeatable_cli_paths() {
    let target = workspace_root().unwrap().join("target");
    fs::create_dir_all(&target).unwrap();
    let workspace = tempfile::tempdir_in(target).unwrap();
    for game in Game::ALL {
        let roots = std::thread::scope(|scope| {
            let first = scope.spawn(|| checkout_in(workspace.path(), game).unwrap());
            let second = scope.spawn(|| checkout_in(workspace.path(), game).unwrap());
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(roots[0], roots[1]);
        assert_original(&roots[0], game);
        let head_path = roots[0].join(".git/HEAD");
        let modified = fs::metadata(&head_path).unwrap().modified().unwrap();
        let path = Command::new(env!("CARGO_BIN_EXE_original-games"))
            .arg("--workspace")
            .arg(workspace.path())
            .args(["path", game.name()])
            .output()
            .unwrap();
        assert!(path.status.success(), "{path:?}");
        assert_eq!(
            String::from_utf8(path.stdout).unwrap().trim(),
            roots[0].to_str().unwrap()
        );
        assert_eq!(
            fs::metadata(head_path).unwrap().modified().unwrap(),
            modified
        );
    }
    let all = Command::new(env!("CARGO_BIN_EXE_original-games"))
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fetch-all")
        .output()
        .unwrap();
    assert!(all.status.success(), "{all:?}");
    let paths = String::from_utf8(all.stdout).unwrap();
    assert_eq!(paths.lines().count(), Game::ALL.len());
    for (game, path) in Game::ALL.into_iter().zip(paths.lines()) {
        assert_eq!(
            Path::new(path),
            checkout_in(workspace.path(), game).unwrap()
        );
    }
}
