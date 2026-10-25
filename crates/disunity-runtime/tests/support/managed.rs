use std::{path::Path, process::Command};

pub fn execute(command: &mut Command) -> String {
    let output = command
        .output()
        .expect("verification tool must be installed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{command:?}:\n{stdout}\n{stderr}");
    stdout.into_owned()
}

pub fn build_suites(root: &Path, dotnet: &std::ffi::OsStr, artifacts: &Path) {
    for project in [
        "UnityCompat",
        "CompatibilityBridge",
        "PortableFixture",
        "Disunity.Tests",
    ] {
        build(
            root,
            dotnet,
            artifacts,
            &format!("{project}/{project}"),
            None,
        );
    }
    originals::build(root, dotnet, artifacts);
    build(
        root,
        dotnet,
        artifacts,
        "Compatibility.Tests/Compatibility.Tests",
        None,
    );
}

fn build(
    root: &Path,
    dotnet: &std::ffi::OsStr,
    artifacts: &Path,
    project: &str,
    source: Option<&Path>,
) {
    let mut command = Command::new(dotnet);
    command
        .args(["build", "--nologo", "-m:2"])
        .arg(root.join(format!("csharp/{project}.csproj")))
        .arg("--artifacts-path")
        .arg(artifacts)
        .current_dir(root);
    if project.starts_with("Compatibility.Tests/") {
        command.arg("-p:BuildProjectReferences=false");
    }
    if source.is_none() {
        command.arg("--warnaserror");
    }
    // Original Unity scripts contain serializer-populated fields; preserve their diagnostics.
    if let Some(source) = source {
        command.arg(format!("-p:OriginalGamesRoot={}", source.display()));
    }
    execute(&mut command);
}

#[path = "originals.rs"]
mod originals;
