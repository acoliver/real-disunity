mod manifest;

use crate::{
    files::{self, Result, io},
    model::{Fixture, SourceState},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(io)?;
    if !output.status.success() {
        return Err(format!("git: {}", String::from_utf8_lossy(&output.stderr)));
    }
    if output.stdout.len() > 16_777_216 {
        return Err("git output cap exceeded".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
pub fn load(root: &Path, name: &str) -> Result<Fixture> {
    let requested = Path::new(name);
    let path = if requested.extension().is_some_and(|ext| ext == "json") {
        if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            root.join(requested)
        }
    } else {
        files::relative_path(requested)?;
        if requested.components().count() != 1 {
            return Err("fixture name must be one path component".into());
        }
        let path = root.join("fixtures").join(format!("{name}.json"));
        if !path.exists() {
            return catalog(root)?
                .into_iter()
                .find(|fixture| fixture.name == name)
                .ok_or_else(|| format!("unknown fixture: {name}"));
        }
        path
    };
    let fixture: Fixture = files::read_json(&path)?;
    if fixture.schema_version != 1 {
        return Err("unsupported fixture schema".into());
    }
    if fixture.name.is_empty()
        || fixture.name.len() > 128
        || !fixture
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("fixture name must contain letters, digits, hyphens or underscores".into());
    }
    fixture.limits.validate()?;
    if fixture.profiles.is_empty() || !fixture.profiles.contains(&fixture.default_profile) {
        return Err("fixture default profile must exist".into());
    }
    crate::reporting::Selection::from_fixture(&fixture)?;
    Ok(fixture)
}
pub fn catalog(root: &Path) -> Result<Vec<Fixture>> {
    let mut fixtures = Vec::new();
    for entry in fs::read_dir(root.join("fixtures")).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.extension().is_some_and(|ext| ext == "json") && !manifest::is_traceability(&path)? {
            fixtures.push(load(root, path.to_str().ok_or("non UTF8 fixture path")?)?);
        }
    }
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    if fixtures.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err("duplicate fixture display names in catalog".into());
    }
    Ok(fixtures)
}
pub fn source_root(root: &Path, fixture: &Fixture) -> Result<PathBuf> {
    let lock_path = root
        .join(".work/compat/locks")
        .join(format!("{}.json", fixture.name));
    if lock_path.exists() {
        let lock: serde_json::Value = files::read_json(&lock_path)?;
        let source = lock["source_root"]
            .as_str()
            .ok_or("invalid fixture lock source_root")?;
        return files::work_path(root, Path::new(source));
    }
    let source = if fixture.source_root.is_absolute() {
        fixture.source_root.clone()
    } else {
        root.join(&fixture.source_root)
    };
    source
        .canonicalize()
        .map_err(|e| format!("source unavailable: {}: {e}", source.display()))
}
pub fn inspect(source: &Path) -> Result<SourceState> {
    let files = files::source_files(source, 100_000_000_000, 1_000_000)?;
    let mut scripts = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    for path in files {
        inputs.insert(
            path.strip_prefix(source)
                .map_err(io)?
                .to_string_lossy()
                .into_owned(),
            files::hash(&path)?,
        );
        if path.extension().is_some_and(|ext| ext == "cs") {
            scripts.insert(
                path.strip_prefix(source)
                    .map_err(io)?
                    .to_string_lossy()
                    .into_owned(),
                files::hash(&path)?,
            );
        }
    }
    let owns_repository = git(source, &["rev-parse", "--show-toplevel"])
        .is_ok_and(|top| Path::new(&top).canonicalize().ok() == source.canonicalize().ok());
    Ok(SourceState {
        root: source.to_path_buf(),
        revision: owns_repository
            .then(|| git(source, &["rev-parse", "HEAD"]).ok())
            .flatten(),
        dirty: owns_repository
            .then(|| {
                git(source, &["status", "--porcelain", "--untracked-files=all"])
                    .ok()
                    .map(|s| !s.is_empty())
            })
            .flatten(),
        scripts,
        inputs,
    })
}
pub fn check(root: &Path, fixture: &Fixture) -> Result<serde_json::Value> {
    let source = source_root(root, fixture)?;
    let state = inspect(&source)?;
    let editor = fs::read_to_string(source.join("ProjectSettings/ProjectVersion.txt")).ok();
    Ok(
        serde_json::json!({"fixture":fixture.name,"source":state,"editor":editor,"profiles":fixture.profiles,"metadata":fixture.metadata}),
    )
}
