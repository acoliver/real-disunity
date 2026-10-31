//! The render pipeline assets of a player's graphics settings.
//!
//! `GraphicsSettings` names the pipeline asset a player starts with and the pipeline
//! package's global settings asset, and each `QualitySettings` level can name its own
//! pipeline asset. [`admit_graphics_assets`] adds those objects to the resources plan as
//! script assets, planned like any other (see [`crate::player_resources`]), and returns their
//! asset indexes. An asset the engine does not build, or that cannot be found, is a plan
//! difference and has no index.

use std::path::Path;

use serde::Serialize;
use worldrunner_unity_serialized::{
    graphics_settings::GraphicsSettings,
    managers::QualitySettings,
    object::{Layout, PPtr},
};

use crate::{PlayerError, player_resources::Registry};

/// The planned assets of a player's render pipeline settings, by asset index.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct GraphicsAssets {
    /// The pipeline asset `GraphicsSettings` names.
    pub render_pipeline: Option<usize>,
    /// The pipeline package's global settings asset.
    pub global_settings: Option<usize>,
    /// The pipeline asset of each quality level in level order; `None` for a level that names
    /// none.
    pub quality_pipelines: Vec<Option<usize>>,
    /// The shaders every build keeps, which `Shader.Find` finds by name, in settings order. A
    /// reference that does not plan is a difference and has no entry.
    pub always_included_shaders: Vec<usize>,
}

/// Admits the pipeline assets that `globalgamemanagers` names to `registry`.
///
/// # Errors
/// Fails when `globalgamemanagers` cannot be read or its graphics or quality settings do not
/// decode.
pub(crate) fn admit_graphics_assets(
    registry: &mut Registry,
    managers: &Path,
) -> Result<GraphicsAssets, PlayerError> {
    let mut assets = GraphicsAssets::default();
    if let Some(graphics) = decode_manager::<GraphicsSettings>(registry, managers)? {
        assets.render_pipeline = admit(
            registry,
            managers,
            graphics.render_pipeline,
            "render pipeline asset",
        );
        assets.global_settings = admit(
            registry,
            managers,
            graphics.global_settings,
            "render pipeline global settings asset",
        );
        assets.always_included_shaders = graphics
            .always_included_shaders
            .iter()
            .filter_map(|&shader| admit(registry, managers, shader, "always included shader"))
            .collect();
    }
    if let Some(quality) = decode_manager::<QualitySettings>(registry, managers)? {
        for pipeline in quality.render_pipelines {
            let asset = admit(
                registry,
                managers,
                pipeline,
                "quality level's render pipeline asset",
            );
            assets.quality_pipelines.push(asset);
        }
    }
    Ok(assets)
}

fn decode_manager<T: Layout>(
    registry: &mut Registry,
    managers: &Path,
) -> Result<Option<T>, PlayerError> {
    let open = registry.files.open_path(managers)?;
    let Some(path_id) = open
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == T::CLASS)
        .map(|object| object.path_id)
    else {
        return Ok(None);
    };
    open.decode(path_id).map(Some)
}

/// The asset index of the object `pptr` names, which `what` describes. A null reference has
/// none and needs no note.
fn admit(registry: &mut Registry, managers: &Path, pptr: PPtr, what: &str) -> Option<usize> {
    if pptr.is_null() {
        return None;
    }
    let place = format!("object {} of file {}", pptr.path_id, pptr.file_id);
    let admitted = (|| {
        let file = registry.files.referenced_file(managers, pptr)?;
        let class = registry.class(&file, pptr.path_id)?;
        let asset = registry.admit(&(file, pptr.path_id), class)?;
        Ok::<_, PlayerError>((class, asset))
    })();
    match admitted {
        Ok((_, Some(asset))) => Some(asset),
        Ok((class, None)) => {
            registry.notes.push(format!(
                "the {what}, {place}, is a {class}, which the engine does not build"
            ));
            None
        }
        Err(error) => {
            registry
                .notes
                .push(format!("the {what}, {place}, cannot be read: {error}"));
            None
        }
    }
}
