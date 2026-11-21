//! The render settings of a player scene as the scene plan's environment.
//!
//! A scene stores one `RenderSettings` object. The plan carries its values in one environment:
//! - the ambient light (a mode, three colours, an intensity and the 27 coefficients of its probe)
//!   and the default reflection (a mode, an intensity, a custom cubemap and the cubemap baked from
//!   the skybox), which the world's environment record holds. Each cubemap is a scene asset with its
//!   pixels named in place (see [`crate::player_layered_textures::plan_scene_cubemap`]), so the
//!   managed loader builds the `Cubemap` and gives the world the identity it stores it under;
//! - the values the managed `RenderSettings` keeps itself: fog, the subtractive shadow colour,
//!   reflection bounces and resolution, halo and flare strength, flare fade speed, the sun as the
//!   planned light and the skybox as the planned material.
//!
//! A scene with no render settings has no environment, and the engine keeps its own defaults for it.
//!
//! A reference the engine cannot plan binds `null` and is listed in the scene's differences: a
//! reference to something that is not a cubemap (a custom reflection may also be a render
//! texture), a cubemap that is not six square faces, a cubemap in a file the player lacks, a sun
//! that is not a light of the plan and a skybox material the plan does not already hold (the plan
//! holds the materials its renderers draw with). Cubemaps the settings hold beyond those two, the
//! halo texture, the spot cookie and the indirect specular colour are not planned.

use std::path::Path;

use serde::Serialize;
use worldrunner_unity_serialized::{ClassId, object::PPtr, render_settings::RenderSettings};

use crate::{
    PlayerError,
    plan::{PlanAsset, PlanComponent, PlanObject},
    player_layered_textures::plan_scene_cubemap,
    player_plan::PlayerPlanner,
    player_shader_refs::{SceneSource, class_of},
    player_values::not_held,
};

/// The scene's environment. `ambient_mode` is Unity's `AmbientMode` (0 skybox, 1 trilight, 3
/// flat, 4 custom), `reflection_mode` its `DefaultReflectionMode` (0 skybox, 1 custom) and
/// `fog_mode` its `FogMode` (1 linear, 2 exponential, 3 exponential squared); modes the player
/// stores outside these pass through as its integers. Colours are `[r, g, b, a]` as the player
/// stored them. A cube is the index of a planned `Cubemap` asset, `skybox_material` the index of a
/// planned `Material` asset and `sun` the `[object, component]` of a planned `Light`.
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
    pub reflection_bounces: i32,
    pub reflection_resolution: i32,
    pub fog_enabled: bool,
    pub fog_color: [f32; 4],
    pub fog_mode: i32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub subtractive_shadow_color: [f32; 4],
    pub halo_strength: f32,
    pub flare_strength: f32,
    pub flare_fade_speed: f32,
    /// `None` for no sun, or one the plan cannot hold (then listed in the differences).
    pub sun: Option<[usize; 2]>,
    /// `None` for no skybox material, or one the plan does not hold (then listed in the
    /// differences).
    pub skybox_material: Option<usize>,
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

    /// The `[object, component]` of the planned light `sun` names, or `None` for no sun, and why
    /// when it is not a light the plan holds.
    fn planned_sun(
        &mut self,
        planner: &PlayerPlanner<'_>,
        objects: &[PlanObject],
        sun: PPtr,
    ) -> Option<[usize; 2]> {
        if sun.is_null() {
            return None;
        }
        let light = planner
            .planned_component(sun)
            .filter(|&[object, component]| {
                matches!(
                    objects
                        .get(object)
                        .and_then(|o| o.components.get(component)),
                    Some(PlanComponent::Light { .. })
                )
            });
        if light.is_none() {
            self.differences
                .push("render settings: the sun is not a light the plan holds".to_owned());
        }
        light
    }

    /// The asset index of the material `skybox` names, as seen from the file `owner`, when the
    /// plan already holds it; `None` for no skybox material, and why when the plan lacks it.
    fn planned_skybox_material(&mut self, owner: &Path, skybox: PPtr) -> Option<usize> {
        if skybox.is_null() {
            return None;
        }
        let planned = self
            .files_mut()
            .referenced_file(owner, skybox)
            .ok()
            .and_then(|file| {
                let key = (file, skybox.path_id);
                self.planned
                    .iter()
                    .find(|(known, _)| *known == key)
                    .map(|&(_, index)| index)
            })
            .filter(|&index| matches!(self.assets[index], PlanAsset::Material { .. }));
        if planned.is_none() {
            self.differences.push(
                "render settings: the skybox material is not an asset the plan holds".to_owned(),
            );
        }
        planned
    }
}

/// Plans the render settings of the scene file `scene`, or `None` when it holds none. Call it
/// after the scene's objects are planned: the sun resolves among `objects`, and the skybox among
/// the assets `source` already holds.
///
/// # Errors
/// Fails when the file cannot be read or the settings do not decode.
pub(crate) fn plan_environment(
    source: &mut SceneSource,
    planner: &PlayerPlanner<'_>,
    scene: &Path,
    objects: &[PlanObject],
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
    let (reflections, ambient, fog) = (settings.reflections, settings.ambient, settings.fog);
    let flares = settings.halo_and_flare;
    let custom = source.reflection_cube(scene, "custom reflection", reflections.custom);
    let skybox = source.reflection_cube(scene, "skybox reflection", reflections.generated_skybox);
    let sun = source.planned_sun(planner, objects, settings.sun);
    let skybox_material = source.planned_skybox_material(scene, settings.skybox_material);
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
        reflection_bounces: reflections.bounces,
        reflection_resolution: reflections.resolution,
        fog_enabled: fog.enabled,
        fog_color: fog.color,
        fog_mode: fog.mode,
        fog_density: fog.density,
        fog_start: fog.linear_start,
        fog_end: fog.linear_end,
        subtractive_shadow_color: settings.subtractive_shadow_color,
        halo_strength: flares.halo_strength,
        flare_strength: flares.flare_strength,
        flare_fade_speed: flares.flare_fade_speed,
        sun,
        skybox_material,
    }))
}
