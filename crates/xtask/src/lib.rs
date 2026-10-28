use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod commands;
mod layers;
mod metadata;
mod policy;
mod scratch;
mod shards;
mod source_size;

fn root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask is not located beneath the workspace root".to_owned())
}

fn prepared(root: &Path, program: &str, args: &[&str]) -> Result<std::process::Command, String> {
    let root = metadata::canonical(root)?;
    let executable =
        std::env::current_exe().map_err(|error| format!("cannot locate running xtask: {error}"))?;
    let executable = metadata::canonical(&executable)?;
    eprintln!("+ {program} {}", args.join(" "));
    Ok(commands::process(&root, program, args, &executable))
}

fn run_command(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let status = prepared(root, program, args)?
        .status()
        .map_err(|error| format!("could not start {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

/// Runs a command like [`run_command`] and returns its standard output. Standard error stays
/// attached to the terminal so cargo progress remains visible in CI logs.
fn capture_command(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let output = prepared(root, program, args)?
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|error| format!("could not start {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} printed non-UTF-8 output: {error}"))
}

fn finish(gate: &str, errors: &[String]) -> Result<(), String> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{gate} checks failed ({}):\n{}",
            errors.len(),
            errors.join("\n")
        ))
    }
}

const USAGE: &str = "cargo xtask <fmt|lint|complexity|check|build|test|policy|source-size|ci|ci-cpu|ci-gpu|ci-gpu-static|ci-managed|ci-gpu-shard K/N|ci-gpu-plan N>";

fn dispatch(root: &Path, args: &[String]) -> Result<(), String> {
    match args {
        [command, value] if matches!(command.as_str(), "ci-gpu-shard" | "ci-gpu-plan") => {
            if command == "ci-gpu-shard" {
                shards::run(root, value)
            } else {
                shards::plan(root, value)
            }
        }
        [command] => dispatch_single(root, command),
        _ => Err(format!("usage: {USAGE}")),
    }
}

fn dispatch_single(root: &Path, command: &str) -> Result<(), String> {
    match command {
        "fmt" => run_command(root, "cargo", &["fmt", "--all", "--check"]),
        command @ ("lint" | "complexity" | "check" | "build" | "test") => {
            commands::standalone(root, command)
        }
        "policy" => policy::check(root),
        "source-size" => source_size::check(root),
        "ci" => commands::ci(root, commands::Profile::Full),
        "ci-cpu" => commands::ci(root, commands::Profile::Cpu),
        "ci-gpu" => commands::ci(root, commands::Profile::Gpu),
        "ci-gpu-static" => commands::ci_gpu_static(root),
        "ci-managed" => commands::ci(root, commands::Profile::Managed),
        "help" | "--help" | "-h" => {
            println!(
                "{USAGE}\nFormatting is check-only. Full CI requires a GPU and .NET; use explicit CI profiles for separate runners.\nci-gpu-static runs every ci-gpu check except tests; ci-gpu-shard K/N runs one shard of the ci-gpu tests; ci-gpu-plan N lists the shards."
            );
            Ok(())
        }
        _ => Err(format!("usage: {USAGE}")),
    }
}

#[must_use]
pub fn run(args: &[String]) -> ExitCode {
    match root().and_then(|root| dispatch(&root, args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_extra_arguments_fail_instead_of_being_ignored() {
        for args in [
            vec![],
            vec!["unknown".into()],
            vec!["policy".into(), "ignored".into()],
            vec!["ci-gpu".into(), "1/2".into()],
            vec!["ci-gpu-static".into(), "ignored".into()],
            vec!["ci-gpu-shard".into()],
            vec!["ci-gpu-plan".into()],
            vec!["ci-gpu-shard".into(), "1/2".into(), "extra".into()],
        ] {
            assert!(dispatch(Path::new("."), &args).is_err());
        }
    }
}
