//! Original gameplay compilation and cached-assembly admission.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use compat_kit::{RunRequest, files};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assembly_probe;

#[derive(Deserialize, Serialize)]
struct Proof {
    assembly: PathBuf,
    assembly_sha256: String,
    source_root: PathBuf,
    source_count: usize,
    source_hashes: BTreeMap<String, String>,
    metadata: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compiler_inputs: Option<Vec<PathBuf>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compiler_inputs_verified: Option<bool>,
}

/// Prepare a gameplay assembly without booting a managed host or executing game code.
pub fn prepare_game(root: &Path, request: &RunRequest) -> Result<PathBuf, String> {
    let build = build_directory(root, request)?;
    let sources = current_sources(request)?;
    if request.metadata["assembly"].is_null() {
        crate::assembly::compile_game(root, request, &build, &sources)
    } else {
        cached(root, request, &build, &sources)
    }
}

pub(crate) fn work_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("assembly and artifact paths must be absolute".into());
    }
    files::work_path(root, path)
}

pub(crate) fn build_directory(root: &Path, request: &RunRequest) -> Result<PathBuf, String> {
    work_path(root, &request.artifact_root)?;
    let build = work_path(root, &request.artifact_root.join("build"))?;
    if build.exists() {
        files::tree_files(&build, 1_073_741_824, 100_000)?;
    }
    fs::create_dir_all(&build).map_err(files::io)?;
    Ok(build)
}

pub(crate) fn current_sources(request: &RunRequest) -> Result<BTreeMap<String, String>, String> {
    if !request.source_root.is_absolute() {
        return Err("source_root must be absolute".into());
    }
    let source = request.source_root.canonicalize().map_err(files::io)?;
    if source != request.source_root {
        return Err("source_root must be canonical and free of symlinks".into());
    }
    for path in request.source_hashes.keys() {
        files::relative_path(Path::new(path))?;
    }
    let directory = source.join("Assets/Scripts");
    if directory.canonicalize().map_err(files::io)? != directory {
        return Err("source directory symlink rejected".into());
    }
    let paths = files::source_files(&directory, 1_073_741_824, 100_000)?;
    let mut actual = BTreeMap::new();
    for path in paths.iter().filter(|path| gameplay_source(path)) {
        let relative = path.strip_prefix(&source).map_err(files::io)?;
        let path = files::safe_file(&source, relative)?;
        actual.insert(relative.to_string_lossy().into_owned(), files::hash(&path)?);
    }
    let selected: BTreeMap<String, String> = request
        .source_hashes
        .iter()
        .filter(|(path, _)| path.starts_with("Assets/Scripts/") && gameplay_source(Path::new(path)))
        .map(|(path, hash)| (path.clone(), hash.clone()))
        .collect();
    if actual.is_empty() {
        return Err("original gameplay compilation has no selected C# files".into());
    }
    if actual.keys().ne(selected.keys()) {
        return Err("current gameplay source selection differs from request".into());
    }
    if actual != selected {
        return Err("current source hashes differ from request".into());
    }
    if let Some(value) = request.metadata.get("script_count") {
        let expected = value
            .as_u64()
            .filter(|count| *count > 0)
            .ok_or("script_count must be a positive integer")?;
        if u64::try_from(actual.len()).map_err(files::io)? != expected {
            return Err(format!(
                "expected {expected} original scripts, found {}",
                actual.len()
            ));
        }
    }
    Ok(actual)
}

fn gameplay_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
}

fn cached(
    root: &Path,
    request: &RunRequest,
    build: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let assembly = request.metadata["assembly"]
        .as_str()
        .ok_or("assembly must be a path string")?;
    let assembly = work_path(root, Path::new(assembly))?;
    let proof_path = request.metadata["assembly_qualification"]
        .as_str()
        .ok_or("cached assembly requires metadata.assembly_qualification")?;
    let proof_path = work_path(root, Path::new(proof_path))?;
    let proof: Proof = files::read_json(&proof_path)?;
    if proof.assembly != assembly || proof.source_root != request.source_root {
        return Err("assembly qualification identity mismatch".into());
    }
    if proof.source_count != sources.len() {
        return Err("assembly qualification source count mismatch".into());
    }
    if proof.source_hashes != *sources {
        return Err("assembly qualification source hashes mismatch".into());
    }
    verify_compiler_inputs(&proof, request, sources)?;
    if assembly_hash(&assembly)? != proof.assembly_sha256 {
        return Err("assembly SHA256 differs from qualification".into());
    }
    let metadata = assembly_probe::inspect(root, build, &assembly)?;
    if metadata != proof.metadata {
        return Err("assembly qualification metadata mismatch".into());
    }
    if current_sources(request)? != *sources || assembly_hash(&assembly)? != proof.assembly_sha256 {
        return Err("assembly or source hashes changed during qualification".into());
    }
    Ok(assembly)
}

fn assembly_hash(assembly: &Path) -> Result<String, String> {
    let metadata = fs::metadata(assembly).map_err(files::io)?;
    if !metadata.is_file() || metadata.len() > 268_435_456 {
        return Err("candidate assembly must be a regular file within the 256 MiB budget".into());
    }
    files::hash(assembly)
}

fn verify_compiler_inputs(
    proof: &Proof,
    request: &RunRequest,
    sources: &BTreeMap<String, String>,
) -> Result<(), String> {
    if proof.compiler_inputs.is_none() && proof.compiler_inputs_verified.is_none() {
        return Ok(());
    }
    let mut actual = proof
        .compiler_inputs
        .clone()
        .ok_or("missing qualified compiler inputs")?;
    actual.sort();
    let expected: Vec<_> = sources
        .keys()
        .map(|path| request.source_root.join(path))
        .collect();
    if proof.compiler_inputs_verified != Some(true) || actual != expected {
        return Err("qualified compiler inputs differ from current source selection".into());
    }
    Ok(())
}

pub(crate) fn record_proof(
    root: &Path,
    request: &RunRequest,
    build: &Path,
    assembly: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<(), String> {
    work_path(root, assembly)?;
    let manifest = fs::read_to_string(build.join("actual-compile-items.txt")).map_err(files::io)?;
    let compiler_inputs: Vec<PathBuf> = manifest
        .trim_start_matches('\u{feff}')
        .lines()
        .map(PathBuf::from)
        .collect();
    let mut actual = compiler_inputs.clone();
    actual.sort();
    let expected: Vec<_> = sources
        .keys()
        .map(|path| request.source_root.join(path))
        .collect();
    if actual != expected {
        return Err("actual MSBuild Compile inputs differ from selected original sources".into());
    }
    let assembly_sha256 = assembly_hash(assembly)?;
    let metadata = assembly_probe::inspect(root, build, assembly)?;
    if current_sources(request)? != *sources || assembly_hash(assembly)? != assembly_sha256 {
        return Err("assembly or source hashes changed during compilation".into());
    }
    let proof = Proof {
        assembly: assembly.into(),
        assembly_sha256,
        source_root: request.source_root.clone(),
        source_count: sources.len(),
        source_hashes: sources.clone(),
        metadata,
        compiler_inputs: Some(compiler_inputs),
        compiler_inputs_verified: Some(true),
    };
    let mut value = serde_json::to_value(proof).map_err(files::io)?;
    value["count"] = serde_json::json!(sources.len());
    value["files"] = serde_json::json!(sources.keys().collect::<Vec<_>>());
    files::write_json(&build.join("original-source-count.json"), &value)
}
