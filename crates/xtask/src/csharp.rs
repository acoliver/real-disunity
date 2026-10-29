//! Tests that consume the C# sources, for pull requests that change only `csharp/`.
//!
//! A pull request whose changes all lie under `csharp/` (or are Markdown) leaves every Rust
//! source unchanged, so only tests that build or read the C# projects can observe it. The CPU
//! and managed profiles cover the packages they select. The remaining consumers are GPU-layer
//! packages, listed in [`CONSUMERS`] and run by `ci-gpu-csharp`. [`check`] fails when a
//! workspace package refers to `csharp/` and no profile or the consumer list covers it, so a
//! new consumer cannot be skipped silently.

use std::collections::BTreeSet;
use std::path::Path;

use crate::commands::{self, Profile};
use crate::metadata::{self, Metadata};

/// GPU-layer packages whose tests build or read the C# projects.
pub const CONSUMERS: [&str; 2] = ["worldrunner", "worldrunner-play"];

/// Whether Rust source text refers to a path inside the repository's `csharp/` directory.
pub fn refers_to_csharp(text: &str) -> bool {
    ["csharp/", "csharp\\", "\"csharp\""]
        .iter()
        .any(|marker| text.contains(marker))
}

fn package_names(selection: &[String]) -> BTreeSet<String> {
    selection
        .windows(2)
        .filter(|pair| pair[0] == "--package")
        .map(|pair| pair[1].clone())
        .collect()
}

fn references(manifest: &Path) -> Result<bool, String> {
    let dir = manifest
        .parent()
        .ok_or_else(|| format!("manifest {} has no directory", manifest.display()))?;
    let mut files = Vec::new();
    crate::policy::rust_files(dir, &mut files)?;
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        if refers_to_csharp(&text) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Packages in `referring` that neither the CPU and managed profiles (`covered`) nor
/// [`CONSUMERS`] run.
pub fn uncovered(referring: &BTreeSet<String>, covered: &BTreeSet<String>) -> Vec<String> {
    referring
        .iter()
        .filter(|name| !covered.contains(*name) && !CONSUMERS.contains(&name.as_str()))
        .cloned()
        .collect()
}

fn referring_packages(data: &Metadata) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    for package in &data.packages {
        if data.workspace_members.contains(&package.id) && references(&package.manifest_path)? {
            found.insert(package.name.clone());
        }
    }
    Ok(found)
}

/// Verifies that every package referring to `csharp/` runs for a C#-only change.
pub fn check(data: &Metadata) -> Result<(), String> {
    let mut covered = package_names(&commands::selection(data, Profile::Cpu)?);
    covered.extend(package_names(&commands::selection(data, Profile::Managed)?));
    let gpu = package_names(&commands::selection(data, Profile::Gpu)?);
    for consumer in CONSUMERS {
        if !gpu.contains(consumer) {
            return Err(format!(
                "C# consumer {consumer} is not a GPU-profile workspace package"
            ));
        }
    }
    let missing = uncovered(&referring_packages(data)?, &covered);
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "packages refer to csharp/ but no C#-only CI job runs their tests: {}; add them to xtask's csharp::CONSUMERS",
        missing.join(", ")
    ))
}

/// The consumer selection with all features, as `ci-gpu` compiles it.
pub fn selection() -> Vec<String> {
    let mut args = Vec::new();
    for consumer in CONSUMERS {
        args.extend(["--package".to_owned(), consumer.to_owned()]);
    }
    args.push("--all-features".into());
    args
}

/// `cargo xtask ci-gpu-csharp`: the GPU-required tests of every C# consumer.
pub fn run(root: &Path) -> Result<(), String> {
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    check(&data)?;
    eprintln!(
        "GPU required mode for C# consumers {}: all tests, including ignored tests; missing adapters are errors",
        CONSUMERS.join(", ")
    );
    let args = commands::cargo_args("test", &selection(), true);
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    crate::run_command(root, "cargo", &refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn csharp_references_are_detected_in_paths_and_joins() {
        assert!(refers_to_csharp(
            r#"root.join("csharp/Disunity.Play/Disunity.Play.csproj")"#
        ));
        assert!(refers_to_csharp(
            r#"include_str!("../../../csharp/Disunity/Input.cs")"#
        ));
        assert!(refers_to_csharp(r#"root.join("csharp").join("Disunity")"#));
        assert!(refers_to_csharp(r"..\csharp\Disunity"));
        assert!(!refers_to_csharp(
            "// C# scripts reach the native world through the ABI"
        ));
        assert!(!refers_to_csharp("let sharpness = 2;"));
    }

    #[test]
    fn uncovered_packages_are_reported_unless_a_profile_or_the_list_covers_them() {
        let referring = set(&[
            "worldrunner-disunity-ffi",
            "worldrunner-play",
            "new-consumer",
        ]);
        let covered = set(&["worldrunner-disunity-ffi"]);
        assert_eq!(
            uncovered(&referring, &covered),
            vec!["new-consumer".to_owned()]
        );
        assert!(uncovered(&set(&["worldrunner"]), &BTreeSet::new()).is_empty());
    }

    #[test]
    fn consumer_selection_compiles_with_all_features_and_runs_ignored_tests_serially() {
        let args = commands::cargo_args("test", &selection(), true);
        for consumer in CONSUMERS {
            assert!(args.windows(2).any(|pair| pair == ["--package", consumer]));
        }
        assert!(args.contains(&"--all-features".into()));
        assert!(args.contains(&"--all-targets".into()));
        assert!(args.ends_with(&[
            "--".into(),
            "--include-ignored".into(),
            "--test-threads=1".into()
        ]));
    }
}
