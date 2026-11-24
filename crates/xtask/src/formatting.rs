//! The formatting check: `cargo fmt --check`, once for each workspace package.
//!
//! `cargo fmt --all` hands rustfmt the root source file of every target of every package in a
//! single command line. Windows refuses a command line over 32,767 characters ("The filename
//! or extension is too long", os error 206), and the workspace outgrew that. One invocation
//! per package keeps each command line a small part of the limit on every platform and checks
//! the same files.

use std::path::Path;

use crate::metadata::{self, Metadata};

/// The `cargo fmt` arguments that together check every workspace package.
fn plan(data: &Metadata) -> Vec<Vec<String>> {
    data.packages
        .iter()
        .filter(|package| data.workspace_members.contains(&package.id))
        .map(|package| {
            ["fmt", "--check", "--package", &package.name]
                .map(str::to_owned)
                .to_vec()
        })
        .collect()
}

/// Checks every package even after one fails, so a run reports all unformatted packages.
pub fn check(root: &Path) -> Result<(), String> {
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    let errors: Vec<String> = plan(&data)
        .iter()
        .filter_map(|args| {
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            crate::run_command(root, "cargo", &refs).err()
        })
        .collect();
    crate::finish("formatting", &errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Stay clear of the 32,767 characters Windows allows, which also cover the program path
    /// and rustfmt's own options.
    const COMMAND_LINE_BUDGET: usize = 30_000;

    /// `cargo metadata` output: the typed form the code under test reads, and the raw form that
    /// also holds the target source paths cargo-fmt passes to rustfmt.
    struct Workspace {
        typed: Metadata,
        raw: Value,
    }

    impl Workspace {
        fn new(raw: Value) -> Self {
            Self {
                typed: serde_json::from_value(raw.clone()).unwrap(),
                raw,
            }
        }

        fn member_packages(&self) -> impl Iterator<Item = &Value> {
            let members = self.raw["workspace_members"].as_array().unwrap();
            self.raw["packages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|package| members.contains(&package["id"]))
        }

        fn member_names(&self) -> Vec<String> {
            let mut names: Vec<String> = self
                .member_packages()
                .map(|package| package["name"].as_str().unwrap().to_owned())
                .collect();
            names.sort();
            names
        }

        /// The length of the command line `cargo fmt` builds for rustfmt from these arguments:
        /// the root source file of every target of the selected packages, space separated.
        fn rustfmt_command_line(&self, args: &[String]) -> usize {
            self.member_packages()
                .filter(|package| selects(args, package["name"].as_str().unwrap()))
                .flat_map(|package| package["targets"].as_array().unwrap())
                .map(|target| target["src_path"].as_str().unwrap().len() + 1)
                .sum()
        }
    }

    /// Whether `cargo <args>` formats the package: `--all` selects every package, otherwise
    /// the ones named by `--package`.
    fn selects(args: &[String], name: &str) -> bool {
        args.iter().any(|arg| arg == "--all")
            || args
                .windows(2)
                .any(|pair| pair[0] == "--package" && pair[1] == name)
    }

    /// A workspace of `packages` packages, each with a library, a binary and three integration
    /// tests under a Windows runner's checkout path.
    fn synthetic(packages: usize) -> Workspace {
        let package = |index: usize| {
            let name = format!("worldrunner-synthetic-package-{index:04}");
            let root = format!("D:\\a\\real-disunity\\real-disunity\\crates\\{name}");
            let target = |relative: &str| {
                serde_json::json!({
                    "name": relative,
                    "kind": ["lib"],
                    "src_path": format!("{root}\\{relative}.rs"),
                })
            };
            serde_json::json!({
                "id": name,
                "name": name,
                "manifest_path": format!("{root}\\Cargo.toml"),
                "dependencies": [],
                "targets": [
                    target("src\\lib"),
                    target("src\\main"),
                    target("tests\\first_integration_test"),
                    target("tests\\second_integration_test"),
                    target("tests\\third_integration_test"),
                ],
            })
        };
        let all: Vec<_> = (0..packages).map(package).collect();
        let ids: Vec<_> = all.iter().map(|package| package["id"].clone()).collect();
        Workspace::new(serde_json::json!({
            "packages": all,
            "workspace_members": ids,
            "workspace_root": "D:\\a\\real-disunity\\real-disunity",
            "resolve": { "nodes": [] },
        }))
    }

    fn this_workspace() -> Workspace {
        let root = crate::root().unwrap();
        let output = std::process::Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--all-features",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success());
        Workspace::new(serde_json::from_slice(&output.stdout).unwrap())
    }

    fn assert_fits_and_covers(workspace: &Workspace) {
        let plan = plan(&workspace.typed);
        for args in &plan {
            let length = workspace.rustfmt_command_line(args);
            assert!(
                length < COMMAND_LINE_BUDGET,
                "`cargo {}` hands rustfmt {length} characters of file names, over {COMMAND_LINE_BUDGET}",
                args.join(" ")
            );
        }
        let checked: Vec<String> = workspace
            .member_names()
            .into_iter()
            .filter(|name| plan.iter().any(|args| selects(args, name)))
            .collect();
        assert_eq!(checked, workspace.member_names());
    }

    #[test]
    fn a_large_workspace_is_checked_without_an_oversized_rustfmt_command_line() {
        let workspace = synthetic(300);
        assert!(workspace.rustfmt_command_line(&["--all".into()]) > 32_767);
        assert_fits_and_covers(&workspace);
    }

    #[test]
    fn this_workspace_is_checked_without_an_oversized_rustfmt_command_line() {
        assert_fits_and_covers(&this_workspace());
    }
}
