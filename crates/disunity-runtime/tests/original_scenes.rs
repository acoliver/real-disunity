//! Loads Pong and Breakout from their original scenes and plays them headless.
//!
//! The games come from `original-games` at their pinned revisions, their scenes from
//! `unity-import plan`, and their scripts are compiled unchanged against Disunity. The tools run
//! as commands so this managed-layer test does not depend on them as crates.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Not canonicalized: on Windows that yields a `\\?\` path, which stops MSBuild resolving the
/// game projects' relative imports.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target(root: &Path) -> PathBuf {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    if target.is_absolute() {
        target
    } else {
        root.join(target)
    }
}

/// Runs `command` and returns its standard output.
fn run(command: &mut Command) -> String {
    let output = command
        .output()
        .expect("verification tool must be installed");
    assert!(
        output.status.success(),
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

fn cargo(root: &Path, args: &[&str]) -> String {
    run(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["run", "--locked", "--quiet", "--jobs", "2"])
            .args(args)
            .current_dir(root),
    )
}

fn dotnet() -> Command {
    Command::new(std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into()))
}

/// Plans `game`'s scene and compiles its scripts into `output`, returning the test argument
/// `game=plan=scripts`.
fn prepare(root: &Path, output: &Path, game: &str, project: &str, scene: &str) -> String {
    let checkout = cargo(
        root,
        &["-p", "worldrunner-original-games", "--", "path", game],
    );
    let checkout = checkout.trim();
    let plan = cargo(
        root,
        &[
            "-p",
            "worldrunner-unity-import",
            "--",
            "plan",
            checkout,
            scene,
        ],
    );
    let plan_path = output.join(format!("{game}.json"));
    std::fs::write(&plan_path, plan).expect("write plan");
    let scripts = output.join(game);
    run(dotnet()
        .args(["build", "--warnaserror"])
        .arg(root.join(format!("csharp/OriginalGames/{project}/{project}.csproj")))
        .arg(format!("-p:GameRoot={checkout}"))
        .arg("-o")
        .arg(&scripts));
    format!(
        "{game}={}={}",
        plan_path.display(),
        scripts.join(format!("Original.{project}.dll")).display()
    )
}

#[test]
fn pong_and_breakout_load_from_their_scenes_and_play() {
    let root = workspace();
    let output = target(&root).join("scene-tests");
    std::fs::create_dir_all(&output).expect("scene test output");
    run(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
            ])
            .current_dir(&root),
    );
    let pong = prepare(&root, &output, "pong", "Pong", "Assets/Scenes/Main.unity");
    let breakout = prepare(
        &root,
        &output,
        "breakout",
        "Breakout",
        "Assets/Scenes/SampleScene.unity",
    );
    let library = target(&root).join("debug").join(format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let passed = run(dotnet()
        .args(["run", "--project"])
        .arg(root.join("csharp/Disunity.SceneTests/Disunity.SceneTests.csproj"))
        .arg("-p:TreatWarningsAsErrors=true")
        .arg("--")
        .arg(library)
        .args([pong, breakout]));
    println!("{passed}");
    assert_eq!(passed.matches("PASS:").count(), 3, "{passed}");
}
