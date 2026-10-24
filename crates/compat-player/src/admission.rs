use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use worldrunner::compat_content::{AdmissionPolicy, ImportProfile, ImportSidecar, ImportedScene};
use worldrunner::managed::CompatibilityHost;

pub(crate) fn import(root: &Path, metadata: &Value) -> Result<ImportedScene, String> {
    let scene = metadata["scene"]
        .as_str()
        .ok_or("fixture metadata has no scene")?;
    let profile: ImportProfile = configuration(root, &metadata["import_profile"])?;
    let sidecar: ImportSidecar = configuration(root, &metadata["compatibility_sidecar"])?;
    worldrunner::compat_content::import_project(root, Path::new(scene), &profile, &sidecar)
        .map_err(|error| error.to_string())
}

pub(crate) fn configuration<T: DeserializeOwned + Default>(
    root: &Path,
    value: &Value,
) -> Result<T, String> {
    if value.is_null() {
        return Ok(T::default());
    }
    let value = if let Some(path) = value.as_str() {
        let path = root.join(path);
        serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?,
        )
        .map_err(|error| error.to_string())?
    } else {
        value.clone()
    };
    serde_json::from_value(value).map_err(|error| error.to_string())
}

pub(crate) fn materialize(
    host: &CompatibilityHost,
    world: u64,
    graph: &ImportedScene,
    policy: &AdmissionPolicy,
) -> Result<BTreeMap<String, u64>, String> {
    let result = worldrunner::compat_content::materialize(graph, policy, |mut value: Value| {
        value["version"] = json!(1);
        host.request(world, &value)
            .map_err(|error| error.to_string())
    })
    .map_err(|error| error.to_string())?;
    Ok(result.handles)
}
