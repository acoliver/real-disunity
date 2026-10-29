use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub workspace_members: Vec<String>,
    pub workspace_root: PathBuf,
    pub resolve: Resolve,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub dependencies: Vec<Dependency>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct Resolve {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub deps: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
pub struct Edge {
    pub pkg: String,
}

pub fn read(root: &Path, args: &[&str]) -> Result<Metadata, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid cargo metadata: {e}"))?;
    if metadata.workspace_members.is_empty() || metadata.resolve.nodes.is_empty() {
        return Err("cargo metadata returned an empty workspace or dependency graph".into());
    }
    Ok(metadata)
}

pub fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", path.display()))
}

pub fn facade_graph(root: &Path) -> Result<Metadata, String> {
    let probe = crate::scratch::Directory::new("facade-graph")?;
    let path = canonical(&root.join("crates/worldrunner"))?;
    let path = toml::Value::String(path.to_string_lossy().into_owned());
    probe.write(
        "Cargo.toml",
        &format!(
            "[workspace]\n[package]\nname = \"quality-facade-probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nworldrunner = {{ path = {path}, default-features = false }}\n"
        ),
    )?;
    probe.write("src/lib.rs", "pub use worldrunner::*;\n")?;
    // A separate consumer avoids workspace-wide feature unification enabling graphics.
    read(probe.path(), &["--offline", "--no-default-features"])
}
