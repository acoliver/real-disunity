//! The sun Play lights its retained scene with, chosen from the lights of the presented frame.
//!
//! The choices here are the engine's own. The main light is the first directional light in the
//! frame's component order: the managed `RenderSettings.sun` and ambient values live only in
//! managed memory, and the host does not read them. The sun shines toward the scene along the
//! light's forward axis. Its irradiance is the light's intensity, at most
//! [`MAXIMUM_SUN_IRRADIANCE_MICRO`]; the scene sun has no colour, so the light's colour is not
//! used. A light that casts shadows gives the fixed [`shadow_settings`]: the light's own bias
//! values are not used, because the engine defines no unit for them.
use glam::Vec3;
use worldrunner_disunity_runtime::PresentationLight;
use worldrunner_render_plan::{
    MAXIMUM_SUN_IRRADIANCE_MICRO, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor,
    SceneColorGrading, SceneDirection, SceneEffects, SceneEffectsParts, SceneShadowCascades,
    SceneSky, SceneSun,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_render_scene::shadows::{CascadeRefreshPolicy, DirectionalShadowSettings};
use worldrunner_scene_viewer::CameraShadowPolicy;
use worldrunner_world_simulation::presentation::{LightKind, LightShadowMode};

use crate::player_meshes::round_to_i64;

/// One in micro-units, the scale of scene directions and irradiance.
const MICRO: f64 = 1_000_000.0;
/// A forward axis shorter than this shines in no direction.
const SHORTEST_AXIS: f64 = 1e-6;

/// The light the scene sun follows: the first directional light the frame shows.
pub(crate) fn main_light(lights: &[PresentationLight]) -> Option<&PresentationLight> {
    lights
        .iter()
        .find(|light| light.settings.kind == LightKind::Directional)
}

/// The scene sun `light` gives, or why it gives none: a light whose forward axis has no length
/// shines in no direction.
pub(crate) fn scene_sun(light: &PresentationLight) -> Result<SceneSun, String> {
    let toward = [8, 9, 10].map(|at| -f64::from(light.local_to_world[at]));
    let length = toward.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length < SHORTEST_AXIS {
        return Err("the main light's forward axis has no direction".into());
    }
    let mut micro = [0_i32; 3];
    for (slot, part) in micro.iter_mut().zip(toward) {
        *slot = round_to_i64(part / length * MICRO)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or("the main light's direction is out of range")?;
    }
    let direction = SceneDirection::new(micro)
        .map_err(|error| format!("the main light's direction: {error}"))?;
    let irradiance = round_to_i64(f64::from(light.settings.intensity) * MICRO)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or("the main light's intensity is out of range")?;
    Ok(SceneSun {
        direction,
        irradiance_micro: irradiance.min(MAXIMUM_SUN_IRRADIANCE_MICRO),
    })
}

/// The shadow settings the viewer starts with when `light` casts shadows: a 2048-texel map, a
/// depth bias of 0.0005 and a normal bias of 2 cm, about the scene origin.
pub(crate) fn shadow_settings(light: &PresentationLight) -> Option<DirectionalShadowSettings> {
    (light.settings.shadows != LightShadowMode::None).then(|| DirectionalShadowSettings {
        focus_meters: Vec3::ZERO,
        half_extent_meters: 50.0,
        depth_half_extent_meters: 100.0,
        resolution: 2048,
        depth_bias: 0.0005,
        normal_bias_meters: 0.02,
        refresh_policy: CascadeRefreshPolicy::default(),
    })
}

/// A white sky, so the ambient light has no colour of its own.
const WHITE: SceneColor = SceneColor {
    red: 255,
    green: 255,
    blue: 255,
};

/// The scene effects of a frame lit by `sun`: the sun, an even ambient light of 0.3 from above
/// and 0.15 from below, three shadow cascades split 10 m, 40 m and 150 m from the camera, and
/// no SSAO, post-processing or anti-aliasing. The sky only colours the ambient light: the scene
/// asks for no atmosphere, so the game's clear colour stays the background.
pub(crate) fn lit_effects(sun: SceneSun, camera: SceneCamera) -> Result<SceneEffects, String> {
    let shadows = SceneShadowCascades::camera_splits(vec![10_000, 40_000, 150_000], 100_000)
        .map_err(|error| format!("the shadow cascades: {error}"))?;
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: WHITE,
            horizon: WHITE,
        },
        sun,
        ambient: SceneAmbient {
            sky_irradiance_micro: 300_000,
            ground_irradiance_micro: 150_000,
        },
        shadows,
        ssao: None,
        camera,
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })
    .map_err(|error| format!("the lit scene effects: {error}"))
}

/// The lighting Play has given the viewer, kept so that a frame sends new scene effects only
/// when its lighting changes.
pub(crate) struct SceneLighting {
    camera: SceneCamera,
    sent: Option<SceneEffects>,
    /// Whether the viewer started with shadow resources, which it cannot add later.
    shadowed: bool,
    /// The camera shadow policy the viewer last accepted.
    policy: Option<CameraShadowPolicy>,
}

impl SceneLighting {
    /// The lighting of the first frame, and the shadow settings the viewer starts with: some
    /// only when the frame's main light casts shadows.
    pub(crate) fn start(
        lights: &[PresentationLight],
        camera: SceneCamera,
    ) -> Result<(Self, Option<DirectionalShadowSettings>), String> {
        let shadows = main_light(lights).and_then(shadow_settings);
        let mut lighting = Self {
            camera,
            sent: None,
            shadowed: shadows.is_some(),
            policy: None,
        };
        lighting.update(lights)?;
        Ok((lighting, shadows))
    }

    /// The scene effects the viewer was last given, none while no frame has been lit.
    pub(crate) fn effects(&self) -> Option<&SceneEffects> {
        self.sent.as_ref()
    }

    /// The scene effects to send for a frame's lights, or none when they are the ones the
    /// viewer already has. Once a frame has been lit, a frame without a directional light keeps
    /// the last sun's direction with no irradiance, so the ambient light stays.
    pub(crate) fn update(
        &mut self,
        lights: &[PresentationLight],
    ) -> Result<Option<SceneEffects>, String> {
        let sun = match (main_light(lights), &self.sent) {
            (Some(light), _) => scene_sun(light)?,
            (None, Some(sent)) => SceneSun {
                irradiance_micro: 0,
                ..*sent.sun()
            },
            (None, None) => return Ok(None),
        };
        let effects = lit_effects(sun, self.camera)?;
        if self.sent.as_ref() == Some(&effects) {
            return Ok(None);
        }
        self.sent = Some(effects.clone());
        Ok(Some(effects))
    }
}

impl SceneLighting {
    /// The camera shadow policy to publish for the presented camera's renderShadows, or none when
    /// there is nothing to change: the viewer started without shadows (it cannot add them), the
    /// frame reports no setting (the published policy stays), or the viewer has this policy
    /// already. Call [`Self::published`] once the viewer accepts it.
    pub(crate) fn camera_policy(&self, render_shadows: Option<bool>) -> Option<CameraShadowPolicy> {
        if !self.shadowed {
            return None;
        }
        let wanted = if render_shadows? {
            CameraShadowPolicy::Enabled
        } else {
            CameraShadowPolicy::Disabled
        };
        (self.policy != Some(wanted)).then_some(wanted)
    }

    /// Records that the viewer accepted `policy`.
    pub(crate) fn published(&mut self, policy: CameraShadowPolicy) {
        self.policy = Some(policy);
    }
}

/// The pose a frame publishes the camera at: the new pose when the camera moved, otherwise the
/// last published pose when the shadow policy changes, and none when nothing changed or no pose
/// has been published yet.
pub(crate) fn camera_pose_to_publish(
    moved: Option<CameraPose>,
    last: Option<CameraPose>,
    policy: Option<CameraShadowPolicy>,
) -> Option<CameraPose> {
    moved.or(policy.and(last))
}

#[cfg(test)]
#[path = "scene_light_tests.rs"]
mod tests;
