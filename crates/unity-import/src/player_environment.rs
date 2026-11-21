//! The render settings of a player scene as the scene plan's environment.
//!
//! A scene stores one `RenderSettings` object: the ambient light (a mode, three colours, an
//! intensity and the 27 coefficients of its probe) and the default reflection (a mode, an
//! intensity, a custom cubemap and the cubemap baked from the skybox). The plan carries those
//! values and plans each cubemap as a scene asset with its pixels named in place (see
//! [`crate::player_layered_textures::plan_scene_cubemap`]), so the managed loader builds the
//! `Cubemap` and gives the world the identity it stores it under. A scene with no render settings
//! has no environment, and the engine keeps its own defaults for it.
//!
//! A reference the engine cannot plan binds `null` and is listed in the scene's differences: a
//! reference to something that is not a cubemap (a custom reflection may also be a render
//! texture), a cubemap that is not six square faces, and a cubemap in a file the player lacks.
//! The scene's fog, skybox material, sun, halo and flare settings are not planned.

use std::path::Path;

use serde::Serialize;
use worldrunner_unity_serialized::{ClassId, object::PPtr, render_settings::RenderSettings};

use crate::{
    PlayerError,
    plan::PlanAsset,
    player_layered_textures::plan_scene_cubemap,
    player_shader_refs::{SceneSource, class_of},
    player_values::not_held,
};

/// The scene's ambient light and default reflection. `ambient_mode` is Unity's `AmbientMode`
/// (0 skybox, 1 trilight, 3 flat, 4 custom) and `reflection_mode` its `DefaultReflectionMode`
/// (0 skybox, 1 custom). A cube is the index of a planned `Cubemap` asset.
#[derive(Debug, Serialize)]
pub struct PlanEnvironment {
    pub ambient_mode: i32,
    pub ambient_sky: [f32; 4],
    pub ambient_equator: [f32; 4],
    pub ambient_ground: [f32; 4],
    pub ambient_intensity: f32,
    /// Nine coefficients of red, then of green, then of blue.
    pub ambient_probe: [f32; 27],
    pub reflection_mode: i32,
    pub reflection_intensity: f32,
    pub custom_reflection: Option<usize>,
    pub skybox_reflection: Option<usize>,
}

impl SceneSource {
    /// The index of the cubemap `pptr` names, as seen from the file `owner`, planned when it is
    /// new; `None` for a reference to nothing, and why when the engine cannot hold it.
    fn cubemap_asset(&mut self, owner: &Path, pptr: PPtr) -> Result<Option<usize>, String> {
        if pptr.is_null() {
            return Ok(None);
        }
        let file = self
            .files_mut()
            .referenced_file(owner, pptr)
            .map_err(|_| not_held(pptr))?;
        let key = (file, pptr.path_id);
        if class_of(self.files_mut(), &key) != Some(ClassId::CUBEMAP) {
            return Err(format!(
                "references object {} of file {}, which is not a Cubemap",
                pptr.path_id, pptr.file_id
            ));
        }
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| *known == key) {
            return Ok(Some(*index));
        }
        let texture =
            plan_scene_cubemap(self.files_mut(), &key).map_err(|cause| cause.to_string())??;
        Ok(Some(self.push_asset(key, PlanAsset::Cubemap { texture })))
    }

    /// The index of the cubemap `pptr` names, or `None` with a difference line saying why not.
    fn reflection_cube(&mut self, owner: &Path, what: &str, pptr: PPtr) -> Option<usize> {
        match self.cubemap_asset(owner, pptr) {
            Ok(index) => index,
            Err(why) => {
                self.differences
                    .push(format!("render settings: the {what} cubemap: {why}"));
                None
            }
        }
    }
}

/// Plans the render settings of the scene file `scene`, or `None` when it holds none.
///
/// # Errors
/// Fails when the file cannot be read or the settings do not decode.
pub(crate) fn plan_environment(
    source: &mut SceneSource,
    scene: &Path,
) -> Result<Option<PlanEnvironment>, PlayerError> {
    let open = source.files_mut().open_path(scene)?;
    let Some(path_id) = open
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::RENDER_SETTINGS)
        .map(|object| object.path_id)
    else {
        return Ok(None);
    };
    let settings: RenderSettings = open.decode(path_id)?;
    let reflections = settings.reflections;
    let ambient = settings.ambient;
    let custom = source.reflection_cube(scene, "custom reflection", reflections.custom);
    let skybox = source.reflection_cube(scene, "skybox reflection", reflections.generated_skybox);
    Ok(Some(PlanEnvironment {
        ambient_mode: ambient.mode,
        ambient_sky: ambient.sky_color,
        ambient_equator: ambient.equator_color,
        ambient_ground: ambient.ground_color,
        ambient_intensity: ambient.intensity,
        ambient_probe: ambient.probe,
        reflection_mode: reflections.mode,
        reflection_intensity: reflections.intensity,
        custom_reflection: custom,
        skybox_reflection: skybox,
    }))
}
