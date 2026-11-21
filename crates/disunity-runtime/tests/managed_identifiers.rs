use std::path::Path;
use std::process::Command;

#[test]
fn installation_identifier_subprocess_regressions_run_without_native_host() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    let project = root.join("csharp/Disunity.IdentifierTests/Disunity.IdentifierTests.csproj");
    run(Command::new(&dotnet)
        .args(["build", "--warnaserror"])
        .arg(&project));
    run(Command::new(&dotnet)
        .args(["run", "--no-build", "--project"])
        .arg(project));
}

fn run(command: &mut Command) {
    let output = command
        .output()
        .expect("verification tool must be installed");
    assert!(
        output.status.success(),
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
