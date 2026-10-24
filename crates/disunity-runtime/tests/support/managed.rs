use std::{path::Path, process::Command};

pub fn execute(command: &mut Command) -> String {
    let output = command
        .output()
        .expect("managed verification requires the .NET SDK and native prerequisites");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{command:?}:\n{stdout}\n{stderr}");
    assert!(output.status.success(), "{command:?}:\n{stdout}\n{stderr}");
    stdout.into_owned()
}

pub fn build_suites(root: &Path, dotnet: &std::ffi::OsStr, artifacts: &Path) {
    for project in ["UnityCompat", "CompatibilityBridge"] {
        execute(
            Command::new(dotnet)
                .args([
                    "build",
                    "--nologo",
                    "--warnaserror",
                    "--no-incremental",
                    "-m:2",
                ])
                .arg(root.join(format!("csharp/{project}/{project}.csproj")))
                .arg("--artifacts-path")
                .arg(artifacts)
                .current_dir(root),
        );
    }
    qualify_originals(root, dotnet, artifacts);
    execute(
        Command::new(dotnet)
            .args([
                "build",
                "--nologo",
                "--warnaserror",
                "--no-incremental",
                "-m:2",
                "-p:BuildProjectReferences=false",
            ])
            .arg(root.join("csharp/Compatibility.Tests/Compatibility.Tests.csproj"))
            .arg("--artifacts-path")
            .arg(artifacts)
            .current_dir(root),
    );
}

fn qualify_originals(root: &Path, dotnet: &std::ffi::OsStr, artifacts: &Path) {
    // Preserve compiler diagnostics from unchanged source fields populated by serialization.
    for game in ["Snake", "Breakout", "Pong"] {
        execute(
            Command::new(dotnet)
                .args([
                    "build",
                    "--nologo",
                    "--no-incremental",
                    "-m:2",
                    "-p:BuildProjectReferences=false",
                ])
                .arg(root.join(format!(
                    "csharp/Compatibility.Tests/OriginalGames/{game}.csproj"
                )))
                .arg("--artifacts-path")
                .arg(artifacts)
                .current_dir(root),
        );
    }
    execute(
        Command::new("python3")
            .arg(root.join("csharp/Compatibility.Tests/qualify_sources.py"))
            .env("RD_WORKTREE", root)
            .env("DISUNITY_TEST_MANAGED_ROOT", artifacts.parent().unwrap())
            .current_dir(root),
    );
}
