use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod commands;
mod layers;
mod metadata;
mod policy;
mod scratch;
mod source_size;

fn root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask is not located beneath the workspace root".to_owned())
}

fn run_command(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    eprintln!("+ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|error| format!("could not start {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
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

const USAGE: &str = "cargo xtask <fmt|lint|complexity|check|build|test|policy|source-size|ci|ci-cpu|ci-gpu|ci-managed>";

fn dispatch(root: &Path, args: &[String]) -> Result<(), String> {
    if args.len() != 1 {
        return Err(format!("usage: {USAGE}"));
    }
    match args[0].as_str() {
        "fmt" => run_command(root, "cargo", &["fmt", "--all", "--check"]),
        command @ ("lint" | "complexity" | "check" | "build" | "test") => {
            commands::standalone(root, command)
        }
        "policy" => policy::check(root),
        "source-size" => source_size::check(root),
        "ci" => commands::ci(root, commands::Profile::Full),
        "ci-cpu" => commands::ci(root, commands::Profile::Cpu),
        "ci-gpu" => commands::ci(root, commands::Profile::Gpu),
        "ci-managed" => commands::ci(root, commands::Profile::Managed),
        "help" | "--help" | "-h" => {
            println!(
                "{USAGE}\nFormatting is check-only. Full CI requires a GPU and .NET; use explicit CI profiles for separate runners."
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
        ] {
            assert!(dispatch(Path::new("."), &args).is_err());
        }
    }
}
