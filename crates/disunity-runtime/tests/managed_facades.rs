#[path = "support/handshake.rs"]
pub mod support;

use std::process::Command;

#[test]
fn original_source_facades_and_native_lifecycle_suites_execute_in_managed_ci() {
    let output = run(&[]);
    for marker in [
        "PASS original compiler inputs Snake: 8",
        "PASS original compiler inputs Breakout: 7",
        "PASS original compiler inputs Pong: 20",
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

fn run(arguments: &[&str]) -> String {
    let root = worldrunner_original_games::workspace_root().expect("workspace must exist");
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    let artifacts = support::artifacts();
    assert!(
        artifacts.bridge().is_file() && artifacts.probe().is_file(),
        "build must produce the exact bridge and test assemblies"
    );
    support::managed::execute(
        Command::new(&dotnet)
            .arg(artifacts.probe())
            .args(["--artifacts"])
            .arg(&artifacts.native)
            .arg(&artifacts.managed_root)
            .arg(&root)
            .args(arguments)
            .current_dir(&root),
    )
}

#[test]
fn managed_profile_codec_and_fault_suites_use_current_artifacts() {
    for suite in [
        "--nested-movenext-only",
        "--nested-authored-breakout",
        "--nested-original-breakout",
        "--nested-unsupported",
        "--nested-matching",
        "--nested-authorization",
        "--nested-waits",
        "--nested-original-pong",
        "--single-original-pong",
        "--nested-start-only",
        "--byte-limits-boundaries",
        "--byte-limits-copy-fault",
        "--byte-limits-descriptor",
        "--byte-limits-only",
        "--readiness-only",
        "--readiness-missing-provider",
        "--handshake-describe-only",
        "--handshake-load-only",
    ] {
        let output = run(&[suite]);
        assert!(
            output.contains("PASS"),
            "{suite} did not report its assertions: {output}"
        );
    }
    assert!(run(&["--nested-scenario", "0", "3"]).contains("PASS"));
}
