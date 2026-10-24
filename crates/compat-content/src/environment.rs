use crate::{ImportError, ImportProfile, catalog::Project, source_io::error};
use serde_json::Value;
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
};

pub(crate) fn pin(
    project: &mut Project<'_>,
    profile: &ImportProfile,
) -> Result<BTreeSet<String>, ImportError> {
    let paths = paths(&project.root, profile)?;
    for path in &paths {
        let bytes = project.read(path, profile, true)?;
        let record = if Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("meta"))
        {
            Some(
                crate::yaml::parse_yaml_value(
                    std::str::from_utf8(&bytes).map_err(|e| error(path, e.to_string()))?,
                )
                .map_err(|e| error(path, e))?,
            )
        } else if path.starts_with("ProjectSettings/")
            && (bytes.starts_with(b"%YAML") || bytes.starts_with(b"--- !u!"))
        {
            let text = std::str::from_utf8(&bytes).map_err(|e| error(path, e.to_string()))?;
            let docs = crate::yaml::parse_unity_yaml(text, profile.max_documents)
                .map_err(|e| error(path, e))?;
            project.account_documents(docs.len(), profile)?;
            Some(Value::Array(docs.into_iter().map(|d| d.fields).collect()))
        } else {
            None
        };
        if let Some(record) = record {
            project.settings.insert(path.clone(), record);
        }
    }
    Ok(paths)
}

pub(crate) fn verify(
    root: &Path,
    profile: &ImportProfile,
    expected: &BTreeSet<String>,
) -> Result<(), ImportError> {
    if &paths(root, profile)? != expected {
        return Err(error(
            "environment",
            "source inventory changed during import; reimport current sources",
        ));
    }
    Ok(())
}

fn paths(root: &Path, profile: &ImportProfile) -> Result<BTreeSet<String>, ImportError> {
    let mut queue =
        VecDeque::from_iter(["Assets/Scripts", "ProjectSettings", "Packages"].map(String::from));
    let mut paths = BTreeSet::new();
    let mut count = 0;
    while let Some(relative) = queue.pop_front() {
        let path = root.join(&relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && matches!(
                        relative.as_str(),
                        "Assets/Scripts" | "ProjectSettings" | "Packages"
                    ) =>
            {
                continue;
            }
            Err(e) => return Err(error(&relative, e.to_string())),
        };
        if metadata.is_symlink() {
            return Err(error(&relative, "source symlink rejected"));
        }
        count += 1;
        if count > profile.max_catalog_entries {
            return Err(ImportError::Limit("environment entries".into()));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|e| error(&relative, e.to_string()))? {
                let entry = entry.map_err(|e| error(&relative, e.to_string()))?;
                let name = entry.file_name();
                let name = name
                    .to_str()
                    .ok_or_else(|| error(&relative, "source name must be UTF-8"))?;
                queue.push_back(format!("{relative}/{name}"));
            }
        } else if metadata.is_file() {
            paths.insert(relative);
        } else {
            return Err(error(&relative, "source must be a regular file"));
        }
    }
    Ok(paths)
}
