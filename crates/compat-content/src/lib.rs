#![forbid(unsafe_code)]

mod admission;
mod binding;
mod catalog;
mod census;
mod dependency;
pub use dependency::*;
mod environment;
mod graph;
mod materialize;
mod model;
pub use materialize::*;
mod native;
mod overlay;
mod package;
mod receipt;
mod source_io;
pub use receipt::{ContentReceipt, import_receipt, import_receipt_with_source};
pub use source_io::{ContentSource, FileContentSource};
mod prefab;
mod property;
mod refs;
mod sprites;
mod tile_collision;
mod tile_geometry;
mod tiles;
mod translated;
pub use translated::*;
mod ui_native;
pub mod yaml;
pub use model::*;
use std::path::Path;

/// Imports one explicitly selected original scene and its reachable serialized dependencies.
/// Source files are read only. Native handles are assigned by the host after admission.
pub fn import_project(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
) -> Result<ImportedScene, ImportError> {
    import_inner(root, scene, profile, sidecar, None).map(|(graph, _)| graph)
}

type ReadBundle = std::collections::BTreeMap<String, Vec<u8>>;
fn import_inner(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
    source: Option<&dyn ContentSource>,
) -> Result<(ImportedScene, ReadBundle), ImportError> {
    sprites::validate_fallbacks(&sidecar.fallback_sprites)?;
    tile_collision::validate(sidecar)?;
    let scene = catalog::relative(scene)?;
    let mut project =
        catalog::Project::index_with_source(root, profile, source.unwrap_or(&FileContentSource))?;
    let guid = project
        .index
        .iter()
        .find(|(_, source)| source.path == scene)
        .map(|(guid, _)| guid.clone())
        .ok_or_else(|| ImportError::Source {
            path: scene.clone(),
            message: "selected scene has no indexed .meta GUID".into(),
        })?;
    let environment = if source.is_some() {
        Some(environment::pin(&mut project, profile)?)
    } else {
        None
    };
    project.reachable(&guid, profile, sidecar)?;
    if !project.docs.contains_key(&guid) {
        return Err(ImportError::Source {
            path: scene,
            message: "selected scene is not Unity text YAML".into(),
        });
    }
    let expansion = prefab::Expansion::build(&project, &guid, profile, sidecar)?;
    project.verify_current(profile)?;
    if let Some(expected) = environment {
        environment::verify(&project.root, profile, &expected)?;
    }
    let sources = std::mem::take(&mut project.sources);
    Ok((
        graph::build(project, expansion, scene, guid, profile, sidecar),
        sources,
    ))
}

/// Enumerates caller-selected runtime C# sources, including nonserialized helpers.
pub fn script_census(
    root: &Path,
    directory: &Path,
    profile: &ImportProfile,
) -> Result<Vec<ScriptSource>, ImportError> {
    census::scripts(root, directory, profile)
}
