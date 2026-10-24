use super::{json, paths, process};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

type Bindings = Vec<(PathBuf, String)>;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing inspection string: {key}"))
}

pub(super) fn bind_inputs(root: &Path, config_path: &Path) -> Result<Bindings, String> {
    let config = json::parse(&process::read_bounded(config_path)?)?;
    let mut files = BTreeSet::from([config_path.to_path_buf()]);
    for key in ["image", "facade"] {
        let path = PathBuf::from(text(&config, key)?);
        files.insert(path.with_extension("pdb"));
        files.insert(path);
    }
    bind_sources(
        root,
        text(&config, "source_root")?,
        &config["sources"],
        &mut files,
    )?;
    generated_files(
        root,
        Path::new(text(&config, "generated_root")?),
        &mut files,
    )?;
    if !config["facade_sources"].is_null() {
        let facade = &config["facade_sources"];
        bind_sources(root, text(facade, "root")?, &facade["sources"], &mut files)?;
        generated_files(root, Path::new(text(facade, "generated_root")?), &mut files)?;
    }
    let work = root.join(".work/resumed-source-signature-proof");
    for relative in [
        "contained.py",
        "host.py",
        "inspector/bin/Debug/net8.0/Inspector.dll",
        "inspector/bin/Debug/net8.0/Inspector.pdb",
        "inspector/bin/Debug/net8.0/Inspector.deps.json",
        "inspector/bin/Debug/net8.0/Inspector.runtimeconfig.json",
        "inspector/Inspector.csproj",
    ] {
        files.insert(work.join(relative));
    }
    files.insert(root.join("tools/compat-display/run.py"));
    generated_files(root, &work.join("inspector/obj/Debug/net8.0"), &mut files)?;
    for entry in std::fs::read_dir(work.join("inspector")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "cs") {
            files.insert(path);
        }
    }
    files
        .into_iter()
        .map(|path| {
            let path = paths::regular_under(root, &path)?;
            Ok((path.clone(), compat_kit::files::hash(&path)?))
        })
        .collect()
}

pub(super) fn validate(
    bytes: &[u8],
    config_path: &Path,
    bindings: &Bindings,
) -> Result<Value, String> {
    let report = json::parse(bytes)?;
    if report["schema"] != "source-signature-inspection-v1"
        || report["disposition"] != "UNQUALIFIED"
    {
        return Err("inspector cannot grant activation authority".into());
    }
    let config = json::parse(&process::read_bounded(config_path)?)?;
    for (key, input) in [("image_sha256", "image"), ("facade_sha256", "facade")] {
        bound_hash(&report, key, Path::new(text(&config, input)?), bindings)?;
    }
    bound_hash(
        &report,
        "pdb_sha256",
        &Path::new(text(&config, "image")?).with_extension("pdb"),
        bindings,
    )?;
    bound_hash(&report, "config_sha256", config_path, bindings)?;
    let inspector = config_path
        .parent()
        .ok_or("config parent")?
        .join("inspector/bin/Debug/net8.0/Inspector.dll");
    bound_hash(&report, "inspector_sha256", &inspector, bindings)?;
    validate_sources(&report, &config, bindings)?;
    for key in [
        "methods",
        "edges",
        "failures",
        "requirements",
        "type_signatures",
        "field_signatures",
        "assembly_references",
    ] {
        let array = report[key]
            .as_array()
            .ok_or_else(|| format!("missing census: {key}"))?;
        if array.len() > 100_000 {
            return Err("inspection census exceeded".into());
        }
    }
    if report["requirements"].as_array().is_none_or(Vec::is_empty) {
        return Err("missing unqualified host requirements".into());
    }
    Ok(report)
}

fn bound_hash(report: &Value, key: &str, path: &Path, bindings: &Bindings) -> Result<(), String> {
    let expected = bindings
        .iter()
        .find(|(p, _)| p == path)
        .ok_or("unbound inspection input")?;
    if text(report, key)? != expected.1 {
        return Err(format!("inspection binding mismatch: {key}"));
    }
    Ok(())
}

fn validate_census(report: &Value, config: &Value) -> Result<(), String> {
    let sources = config["sources"].as_object().ok_or("source census")?;
    let documents = report["documents"].as_array().ok_or("document census")?;
    let mut seen = BTreeSet::new();
    let mut source_count = 0;
    for document in documents {
        let path = text(document, "path")?;
        paths::validate_document_path(path)?;
        let category = text(document, "category")?;
        if !seen.insert((category, path)) {
            return Err("duplicate inspection document".into());
        }
        if category == "source" {
            if sources.get(path).and_then(Value::as_str) != Some(text(document, "sha256")?) {
                return Err("inspection source/PDB census mismatch".into());
            }
            source_count += 1;
        } else if category != "generated" {
            return Err("unknown source category".into());
        }
    }
    if source_count != sources.len() {
        return Err("incomplete source/PDB census".into());
    }
    Ok(())
}

fn bind_sources(
    root: &Path,
    source_root: &str,
    value: &Value,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let sources = value.as_object().ok_or("missing source census")?;
    if sources.is_empty() || sources.len() > 4096 {
        return Err("invalid source census size".into());
    }
    for (relative, expected) in sources {
        paths::validate_document_path(relative)?;
        let path = paths::regular_under(root, &Path::new(source_root).join(relative))?;
        if compat_kit::files::hash(&path)? != expected.as_str().ok_or("invalid source hash")? {
            return Err(format!("source census mismatch: {relative}"));
        }
        files.insert(path);
    }
    Ok(())
}

fn generated_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let directory = paths::anchored(root, directory)?;
    let mut pending = vec![(directory, 0)];
    let mut inspected = 0;
    while let Some((directory, depth)) = pending.pop() {
        if depth > 32 {
            return Err("generated directory depth exceeded".into());
        }
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            inspected += 1;
            if inspected > 8192 {
                return Err("generated input census exceeded".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("symlink generated source input".into());
            }
            if kind.is_dir() {
                pending.push((entry.path(), depth + 1));
            } else if entry.path().extension().is_some_and(|e| e == "cs") {
                files.insert(entry.path());
            }
        }
    }
    Ok(())
}

fn validate_sources(report: &Value, config: &Value, bindings: &Bindings) -> Result<(), String> {
    validate_census(report, config)?;
    if !config["facade_sources"].is_null() {
        bound_hash(
            report,
            "facade_pdb_sha256",
            &Path::new(text(config, "facade")?).with_extension("pdb"),
            bindings,
        )?;
        let facade = serde_json::json!({"sources": config["facade_sources"]["sources"]});
        let facade_report = serde_json::json!({"documents": report["facade_documents"]});
        validate_census(&facade_report, &facade)?;
    }
    for key in ["documents", "facade_documents"] {
        let documents = report[key].as_array().ok_or("missing document table")?;
        for document in documents.iter().filter(|d| d["category"] == "generated") {
            let base = if key == "documents" {
                config
            } else {
                &config["facade_sources"]
            };
            let path = Path::new(text(base, "generated_root")?).join(text(document, "path")?);
            bound_hash(document, "sha256", &path, bindings)?;
        }
    }
    Ok(())
}
