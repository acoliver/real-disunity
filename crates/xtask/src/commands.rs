use std::path::Path;
use std::process::Command;

use crate::{layers, metadata, run_command};

const COMPLEXITY: &[&str] = &[
    "-A",
    "clippy::all",
    "-A",
    "clippy::pedantic",
    "-A",
    "clippy::nursery",
    "-D",
    "clippy::cognitive_complexity",
    "-D",
    "clippy::too_many_lines",
    "-D",
    "clippy::too_many_arguments",
    "-D",
    "clippy::type_complexity",
    "-D",
    "clippy::struct_excessive_bools",
];

pub fn process(root: &Path, program: &str, args: &[&str], executable: &Path) -> Command {
    let mut target = root.join("target/validation");
    // A validation-built xtask must not overwrite itself either. Use a sibling,
    // not a nested directory, so repeated launches never grow the target path.
    if executable.starts_with(&target) {
        target = root.join("target/validation-alt");
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target);
    command
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Full,
    Cpu,
    Gpu,
    Managed,
}

pub fn selection(data: &metadata::Metadata, profile: Profile) -> Result<Vec<String>, String> {
    if profile == Profile::Full {
        return Ok(vec!["--workspace".into(), "--all-features".into()]);
    }
    let layers = layers::workspace_layers(data)?;
    let mut args = Vec::new();
    for (package, layer) in layers {
        let include = match profile {
            Profile::Cpu => {
                layer.cpu() || matches!(package.as_str(), "xtask" | "worldrunner-content-tool")
            }
            Profile::Gpu => layer != layers::Layer::Managed,
            Profile::Managed => {
                layer == layers::Layer::Managed || package == "worldrunner-disunity-ffi"
            }
            Profile::Full => unreachable!(),
        };
        if include {
            args.extend(["--package".into(), package]);
        }
    }
    if args.is_empty() {
        return Err(format!(
            "no packages selected for {profile:?}; refusing empty CI"
        ));
    }
    args.push("--all-features".into());
    Ok(args)
}

pub fn cargo_args(command: &str, selection: &[String], gpu_required: bool) -> Vec<String> {
    let cargo = if command == "lint" || command == "complexity" {
        "clippy"
    } else {
        command
    };
    let mut args: Vec<String> = [cargo, "--locked", "--jobs", "2"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    args.extend(selection.iter().cloned());
    if command == "doc" {
        args[0] = "test".into();
        args.push("--doc".into());
    } else {
        args.push("--all-targets".into());
    }
    if command == "lint" {
        args.extend(["--", "-D", "warnings"].map(str::to_owned));
    }
    if command == "complexity" {
        args.push("--".into());
        args.extend(COMPLEXITY.iter().map(|s| (*s).into()));
    }
    if matches!(command, "test" | "doc") && gpu_required {
        args.extend(["--", "--include-ignored", "--test-threads=1"].map(str::to_owned));
    }
    args
}

fn execute(root: &Path, command: &str, selected: &[String], gpu: bool) -> Result<(), String> {
    let args = cargo_args(command, selected, gpu);
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    run_command(root, "cargo", &refs)
}

pub fn standalone(root: &Path, command: &str) -> Result<(), String> {
    let selected = vec!["--workspace".into(), "--all-features".into()];
    execute(root, command, &selected, command == "test")?;
    if command == "test" {
        execute(root, "doc", &selected, true)?;
    }
    Ok(())
}

fn preflight(root: &Path) -> Result<(), String> {
    crate::policy::check(root)?;
    crate::source_size::check(root)?;
    run_command(root, "cargo", &["fmt", "--all", "--check"])
}

const GPU_REQUIRED_NOTICE: &str = "GPU required mode: all selected tests, including ignored tests, run without name filters; missing adapters are errors";

/// Every `ci-gpu` check except the test run, which `ci-gpu-shard` splits across runners.
pub fn ci_gpu_static(root: &Path) -> Result<(), String> {
    preflight(root)?;
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    let selected = selection(&data, Profile::Gpu)?;
    eprintln!("{GPU_REQUIRED_NOTICE}");
    for command in ["lint", "complexity", "check", "build", "doc"] {
        execute(root, command, &selected, true)?;
    }
    Ok(())
}

pub fn ci(root: &Path, profile: Profile) -> Result<(), String> {
    preflight(root)?;
    if profile == Profile::Managed {
        run_command(
            root,
            "cargo",
            &[
                "build",
                "--locked",
                "--jobs",
                "2",
                "--package",
                "worldrunner-disunity-ffi",
            ],
        )?;
        run_command(root, "dotnet", &["--info"])?;
        run_command(
            root,
            "dotnet",
            &[
                "build",
                "csharp/PortableFixture/PortableFixture.csproj",
                "--configuration",
                "Debug",
                "--warnaserror",
            ],
        )?;
    }
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    let selected = selection(&data, profile)?;
    let gpu = matches!(profile, Profile::Full | Profile::Gpu);
    if gpu {
        eprintln!("{GPU_REQUIRED_NOTICE}");
    }
    for command in ["lint", "complexity", "check", "build", "test", "doc"] {
        execute(root, command, &selected, gpu)?;
        if profile == Profile::Cpu {
            execute(
                root,
                command,
                &[
                    "--package".into(),
                    "worldrunner".into(),
                    "--no-default-features".into(),
                ],
                false,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(command: &Command) -> std::path::PathBuf {
        command
            .get_envs()
            .find_map(|(key, value)| (key == "CARGO_TARGET_DIR").then_some(value))
            .flatten()
            .expect("child commands must explicitly share an isolated target directory")
            .into()
    }

    #[test]
    fn child_builds_cannot_replace_the_running_xtask() {
        let root = std::env::temp_dir().join("xtask-command-planning");
        for bootstrap in [
            "target",
            "custom-target",
            "target/validation",
            "target/validation-alt",
        ] {
            let executable = root.join(bootstrap).join("debug/xtask.exe");
            let args = cargo_args("build", &["--package".into(), "xtask".into()], false);
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            let command = process(&root, "cargo", &refs, &executable);
            let output = target(&command);
            let expected = if bootstrap == "target/validation" {
                "target/validation-alt"
            } else {
                "target/validation"
            };
            assert_eq!(output, root.join(expected));
            assert!(output.is_absolute());
            assert!(!executable.starts_with(&output));
            assert_eq!(command.get_current_dir(), Some(root.as_path()));
            assert_eq!(command.get_args().collect::<Vec<_>>(), refs);
            assert!(args.windows(2).any(|pair| pair == ["--package", "xtask"]));
            assert!(args.contains(&"--all-targets".into()));
        }
    }

    #[test]
    fn validation_commands_share_the_target_inherited_by_ffi_builds() {
        let root = std::env::temp_dir().join("xtask-command-planning");
        let executable = root.join("target/debug/xtask.exe");
        let expected = root.join("target/validation");
        for action in ["lint", "complexity", "check", "build", "test", "doc"] {
            let args = cargo_args(action, &[], false);
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            assert_eq!(
                target(&process(&root, "cargo", &refs, &executable)),
                expected
            );
        }
        assert_eq!(
            target(&process(&root, "dotnet", &["build"], &executable)),
            expected
        );
        assert_eq!(
            target(&process(
                &root,
                "cargo",
                &["fmt", "--all", "--check"],
                &executable
            )),
            expected
        );
    }

    #[test]
    fn required_gpu_runs_every_test_including_ignored_without_filters() {
        let args = cargo_args(
            "test",
            &["--workspace".into(), "--all-features".into()],
            true,
        );
        assert!(args.ends_with(&[
            "--".into(),
            "--include-ignored".into(),
            "--test-threads=1".into()
        ]));
        assert!(!args.iter().any(|arg| arg == "--skip"));
        assert!(args.windows(2).any(|pair| pair == ["--jobs", "2"]));
    }

    #[test]
    fn doctests_remain_a_separate_command_and_lints_remain_strict() {
        assert!(cargo_args("doc", &[], false).contains(&"--doc".into()));
        assert!(!cargo_args("doc", &[], false).contains(&"--all-targets".into()));
        assert!(cargo_args("lint", &[], false).ends_with(&[
            "--".into(),
            "-D".into(),
            "warnings".into()
        ]));
        for lint in [
            "cognitive_complexity",
            "too_many_lines",
            "too_many_arguments",
            "type_complexity",
            "struct_excessive_bools",
        ] {
            assert!(cargo_args("complexity", &[], false).contains(&format!("clippy::{lint}")));
        }
    }
}
