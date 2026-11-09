//! The crates a change affects; the hub gate tests only those (board thread 81).
//!
//! The hub gate writes every path the change touches, against its merge base with hub main,
//! to the file `XTASK_CHANGED_FILES` names; the gate computes it, so a change cannot narrow
//! it. Each path maps to the workspace crate whose directory holds it, longest match first,
//! and every crate that depends on one of those, directly or not and dev-dependencies
//! included, is affected too. The shared build configuration, xtask itself and any
//! non-documentation path outside every crate (assets, data, `csharp/`) test everything, as
//! does a run without the variable. Documentation alone runs no Rust tests. Lint and build
//! always cover the whole profile, and the full suite runs every night.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::metadata::Metadata;

const EVERYTHING_FILES: &[&str] = &[
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
    "clippy.toml",
    "rustfmt.toml",
    "deny.toml",
];
const EVERYTHING_DIRS: &[&str] = &[".cargo/", "crates/xtask/"];
const DOC_DIRS: &[&str] = &[
    "docs/",
    "dev-docs/",
    "research/",
    "reports/",
    ".hn/",
    ".github/",
    ".llxprt/",
];

/// The workspace crates by package name: each one's directory relative to the workspace
/// root (ending in `/`) and the workspace crates it depends on.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub dirs: BTreeMap<String, String>,
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Every crate, and why.
    Everything(String),
    /// These crates, by package name.
    Crates(BTreeSet<String>),
    /// No Rust tests: documentation only.
    Nothing,
}

impl Selection {
    pub fn describe(&self, total: usize) -> String {
        match self {
            Self::Everything(reason) => format!("testing everything: {reason}"),
            Self::Crates(names) => format!(
                "affected: {} of {total} crates: {}",
                names.len(),
                names.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            Self::Nothing => "affected: no crates (documentation only); no Rust tests".into(),
        }
    }

    pub fn includes(&self, package: &str) -> bool {
        match self {
            Self::Everything(_) => true,
            Self::Crates(names) => names.contains(package),
            Self::Nothing => false,
        }
    }
}

pub fn graph(data: &Metadata) -> Graph {
    let root = data.workspace_root.to_string_lossy().replace('\\', "/");
    let members: Vec<_> = data
        .packages
        .iter()
        .filter(|p| data.workspace_members.contains(&p.id))
        .collect();
    let names: BTreeSet<&str> = members.iter().map(|p| p.name.as_str()).collect();
    let mut graph = Graph::default();
    for package in members {
        let manifest = package.manifest_path.to_string_lossy().replace('\\', "/");
        let relative = manifest.strip_prefix(&root).unwrap_or(&manifest);
        let dir = Path::new(relative.trim_start_matches('/'))
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        graph.dirs.insert(
            package.name.clone(),
            if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            },
        );
        let internal = package
            .dependencies
            .iter()
            .map(|d| d.name.as_str())
            .filter(|d| names.contains(d))
            .map(str::to_owned)
            .collect();
        graph.dependencies.insert(package.name.clone(), internal);
    }
    graph
}

fn is_doc(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        || DOC_DIRS.iter().any(|d| path.starts_with(d))
}

fn owner<'a>(path: &str, graph: &'a Graph) -> Option<&'a str> {
    graph
        .dirs
        .iter()
        .filter(|(_, dir)| !dir.is_empty() && path.starts_with(dir.as_str()))
        .max_by_key(|(_, dir)| dir.len())
        .map(|(name, _)| name.as_str())
}

pub fn select(changed: &[String], graph: &Graph) -> Selection {
    let paths: Vec<String> = changed
        .iter()
        .map(|p| p.trim().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect();
    for path in &paths {
        if EVERYTHING_FILES.contains(&path.as_str())
            || EVERYTHING_DIRS.iter().any(|d| path.starts_with(d))
        {
            return Selection::Everything(format!("{path} changed"));
        }
        if owner(path, graph).is_none() && !is_doc(path) {
            return Selection::Everything(format!("{path} changed, outside every crate"));
        }
    }
    let mut affected: BTreeSet<String> = paths
        .iter()
        .filter_map(|p| owner(p, graph))
        .map(str::to_owned)
        .collect();
    if affected.is_empty() {
        return Selection::Nothing;
    }
    loop {
        let before = affected.len();
        for (name, deps) in &graph.dependencies {
            if deps.iter().any(|d| affected.contains(d)) {
                affected.insert(name.clone());
            }
        }
        if affected.len() == before {
            return Selection::Crates(affected);
        }
    }
}

/// The selection for this run: from `XTASK_CHANGED_FILES` when the gate set it, otherwise
/// everything.
pub fn from_env(data: &Metadata) -> Selection {
    let Some(file) = std::env::var_os("XTASK_CHANGED_FILES") else {
        return Selection::Everything("no XTASK_CHANGED_FILES (not a hub gate)".into());
    };
    match std::fs::read_to_string(&file) {
        Ok(text) => select(
            &text.lines().map(str::to_owned).collect::<Vec<_>>(),
            &graph(data),
        ),
        Err(error) => Selection::Everything(format!("cannot read XTASK_CHANGED_FILES: {error}")),
    }
}

/// `selected` (cargo package arguments) narrowed to the affected crates, or `None` when no
/// selected crate is affected. `--workspace` and every non-package argument pass through.
pub fn narrow(selected: &[String], selection: &Selection) -> Option<Vec<String>> {
    if matches!(selection, Selection::Everything(_)) {
        return Some(selected.to_vec());
    }
    let mut out = Vec::new();
    let mut packages = 0;
    let mut kept = 0;
    let mut args = selected.iter();
    while let Some(arg) = args.next() {
        if arg == "--package" || arg == "-p" {
            let Some(name) = args.next() else { break };
            packages += 1;
            if selection.includes(name) {
                kept += 1;
                out.extend([arg.clone(), name.clone()]);
            }
        } else if arg == "--workspace" {
            return Some(selected.to_vec());
        } else {
            out.push(arg.clone());
        }
    }
    (packages == 0 || kept > 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_of(edges: &[(&str, &str, &[&str])]) -> Graph {
        let mut g = Graph::default();
        for (name, dir, deps) in edges {
            g.dirs.insert((*name).into(), (*dir).into());
            g.dependencies.insert(
                (*name).into(),
                deps.iter().map(|d| (*d).to_owned()).collect(),
            );
        }
        g
    }

    fn sample() -> Graph {
        graph_of(&[
            ("a", "crates/a/", &[]),
            ("b", "crates/b/", &["a"]),
            ("c", "crates/c/", &[]),
            ("d", "crates/b/tools/d/", &["c"]),
        ])
    }

    fn names(s: &Selection) -> Vec<String> {
        match s {
            Selection::Crates(n) => n.iter().cloned().collect(),
            other => panic!("expected crates, got {other:?}"),
        }
    }

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_change_tests_the_crate_and_everything_that_depends_on_it() {
        assert_eq!(
            names(&select(&paths(&["crates/a/src/lib.rs"]), &sample())),
            ["a", "b"]
        );
        assert_eq!(
            names(&select(&paths(&["crates/c/src/lib.rs"]), &sample())),
            ["c", "d"]
        );
    }

    #[test]
    fn the_longest_crate_directory_owns_a_path() {
        assert_eq!(
            names(&select(&paths(&["crates/b/tools/d/x.rs"]), &sample())),
            ["d"]
        );
    }

    #[test]
    fn shared_configuration_xtask_and_outside_paths_test_everything() {
        for path in [
            "Cargo.lock",
            "Cargo.toml",
            "crates/xtask/src/lib.rs",
            "csharp/A.cs",
            "assets/x.png",
            ".cargo/config.toml",
        ] {
            assert!(
                matches!(select(&paths(&[path]), &sample()), Selection::Everything(_)),
                "{path}"
            );
        }
    }

    #[test]
    fn documentation_alone_tests_nothing() {
        assert_eq!(
            select(
                &paths(&["docs/x.md", "README.md", ".hn/policy.json"]),
                &sample()
            ),
            Selection::Nothing
        );
        assert_eq!(
            names(&select(&paths(&["docs/x.md", "crates/c/a.rs"]), &sample())),
            ["c", "d"]
        );
    }

    #[test]
    fn narrowing_keeps_only_affected_packages_and_other_arguments() {
        let selected = paths(&["--package", "a", "--package", "c", "--all-features"]);
        let only_c = Selection::Crates(["c".to_owned()].into());
        assert_eq!(
            narrow(&selected, &only_c),
            Some(paths(&["--package", "c", "--all-features"]))
        );
        let only_b = Selection::Crates(["b".to_owned()].into());
        assert_eq!(narrow(&selected, &only_b), None);
        assert_eq!(narrow(&selected, &Selection::Nothing), None);
        let all = Selection::Everything("x".into());
        assert_eq!(narrow(&selected, &all), Some(selected.clone()));
        let workspace = paths(&["--workspace", "--all-features"]);
        assert_eq!(narrow(&workspace, &only_b), Some(workspace.clone()));
    }
}
