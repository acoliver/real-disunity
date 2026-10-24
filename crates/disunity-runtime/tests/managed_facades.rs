#[path = "support/handshake.rs"]
mod support;

use std::{path::Path, process::Command};

#[test]
fn original_source_facades_and_native_lifecycle_suites_execute_in_managed_ci() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().expect("workspace must exist");
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    let artifacts = support::artifacts();
    assert!(
        artifacts.bridge().is_file() && artifacts.probe().is_file(),
        "build must produce the exact bridge and test assemblies"
    );
    let output = support::managed::execute(
        Command::new(&dotnet)
            .arg(artifacts.probe())
            .env("RD_WORKTREE", &root)
            .env("DISUNITY_TEST_MANAGED_ROOT", &artifacts.managed_root)
            .env("DISUNITY_TEST_NATIVE_LIBRARY", &artifacts.native)
            .current_dir(&root),
    );
    for marker in [
        "PASS source engineering evidence Snake: 8",
        "PASS source engineering evidence Breakout: 7",
        "PASS source engineering evidence Pong: 20",
        "PASS original DeathZone:",
        "PASS native liveness:",
        "PASS lifecycle resource contract:",
        "PASS contact lifetime: native callback destruction",
        "PASS binder:",
        "PASS contacts:",
        "PASS tile collider:",
        "PASS coroutines:",
        "PASS clone graph:",
        "PASS serialization:",
        "PASS identity lifetime:",
        "PASS Cdecl bridge:",
        "PASS Cdecl disposal:",
    ] {
        assert!(
            output.contains(marker),
            "suite did not execute {marker}:\n{output}"
        );
    }
}
