//! The crates a change affects; the hub gate tests only those (board thread 81).
//!
//! The hub gate writes every path the change touches, against its merge base with hub main,
//! to the file `XTASK_CHANGED_FILES` names; the gate computes it, so a change cannot narrow
//! it. Each path maps to the workspace crate whose directory holds it, longest match first,
//! and every crate that depends on one of those, directly or not and dev-dependencies
//! included, is affected too. A path under `csharp/` selects the packages whose Rust sources
//! refer to `csharp/` (the rule of `csharp::check`, so a new referrer is picked up) and
//! their dependents; when no package refers to `csharp/` it tests everything. The shared
//! build configuration, xtask itself and any other non-documentation path outside every
//! crate (assets, data) test everything, as does a run without the variable. Documentation
//! alone runs no Rust tests. Lint and build always cover the whole profile, and the full
//! suite runs every night.

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
const CSHARP_DIR: &str = "csharp/";
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
    /// The workspace packages whose Rust sources refer to `csharp/`.
    pub csharp_referrers: BTreeSet<String>,
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

pub fn graph(data: &Metadata) -> Result<Graph, String> {
    let root = data.workspace_root.to_string_lossy().replace('\\', "/");
    let members: Vec<_> = data
        .packages
        .iter()
        .filter(|p| data.workspace_members.contains(&p.id))
        .collect();
    let names: BTreeSet<&str> = members.iter().map(|p| p.name.as_str()).collect();
    let mut graph = Graph {
        csharp_referrers: crate::csharp::referring_packages(data)?,
        ..Graph::default()
    };
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
    Ok(graph)
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
    let mut csharp_path = None;
    for path in &paths {
        if EVERYTHING_FILES.contains(&path.as_str())
            || EVERYTHING_DIRS.iter().any(|d| path.starts_with(d))
        {
            return Selection::Everything(format!("{path} changed"));
        }
        if owner(path, graph).is_none() && !is_doc(path) {
            if !path.starts_with(CSHARP_DIR) {
                return Selection::Everything(format!("{path} changed, outside every crate"));
            }
            csharp_path.get_or_insert(path);
        }
    }
    let mut affected: BTreeSet<String> = paths
        .iter()
        .filter_map(|p| owner(p, graph))
        .map(str::to_owned)
        .collect();
    if let Some(path) = csharp_path {
        // Without a known referrer the C# change would select nothing, which proves nothing.
        let referrers: Vec<_> = graph
            .csharp_referrers
            .iter()
            .filter(|name| graph.dirs.contains_key(*name))
            .collect();
        if referrers.is_empty() {
            return Selection::Everything(format!(
                "{path} changed and no package refers to csharp/"
            ));
        }
        affected.extend(referrers.into_iter().cloned());
    }
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
        Ok(text) => match graph(data) {
            Ok(graph) => select(&text.lines().map(str::to_owned).collect::<Vec<_>>(), &graph),
            Err(error) => Selection::Everything(format!("cannot find csharp/ referrers: {error}")),
        },
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

    fn with_csharp_referrers(mut graph: Graph, referrers: &[&str]) -> Graph {
        graph.csharp_referrers = referrers.iter().map(|r| (*r).to_owned()).collect();
        graph
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
            "assets/x.png",
            "data/y.json",
            ".cargo/config.toml",
        ] {
            assert!(
                matches!(select(&paths(&[path]), &sample()), Selection::Everything(_)),
                "{path}"
            );
        }
    }

    #[test]
    fn a_csharp_change_tests_the_referring_packages_and_their_dependents() {
        let graph = with_csharp_referrers(sample(), &["a"]);
        assert_eq!(
            names(&select(
                &paths(&["csharp/Disunity/Input.cs", "csharp/Play/Play.csproj"]),
                &graph
            )),
            ["a", "b"]
        );
    }

    #[test]
    fn a_csharp_change_with_a_crate_change_tests_the_union() {
        let graph = with_csharp_referrers(sample(), &["a"]);
        assert_eq!(
            names(&select(
                &paths(&["csharp/A.cs", "crates/c/src/lib.rs"]),
                &graph
            )),
            ["a", "b", "c", "d"]
        );
    }

    #[test]
    fn a_csharp_change_does_not_soften_the_paths_that_test_everything() {
        let graph = with_csharp_referrers(sample(), &["a"]);
        for other in ["Cargo.lock", "assets/x.png", "crates/xtask/src/lib.rs"] {
            assert!(
                matches!(
                    select(&paths(&["csharp/A.cs", other]), &graph),
                    Selection::Everything(_)
                ),
                "{other}"
            );
            assert!(
                matches!(select(&paths(&[other]), &graph), Selection::Everything(_)),
                "{other}"
            );
        }
    }

    #[test]
    fn a_csharp_change_tests_everything_when_no_package_refers_to_csharp() {
        assert!(matches!(
            select(&paths(&["csharp/A.cs"]), &sample()),
            Selection::Everything(_)
        ));
    }

    #[test]
    fn csharp_documentation_alone_tests_nothing() {
        let graph = with_csharp_referrers(sample(), &["a"]);
        assert_eq!(
            select(&paths(&["csharp/README.md"]), &graph),
            Selection::Nothing
        );
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
