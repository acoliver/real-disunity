//! The `RenderSettings` layout: a scene's fog, ambient light, skybox and reflections.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneFog {
    pub enabled: bool,
    pub color: [f32; 4],
    /// Unity's `FogMode`: 1 linear, 2 exponential, 3 exponential squared.
    pub mode: i32,
    pub density: f32,
    pub linear_start: f32,
    pub linear_end: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientLight {
    pub sky_color: [f32; 4],
    pub equator_color: [f32; 4],
    pub ground_color: [f32; 4],
    pub intensity: f32,
    /// Unity's `AmbientMode`: 0 skybox, 1 trilight, 3 flat, 4 custom.
    pub mode: i32,
    /// The ambient light as Unity's order-2 spherical harmonics, 27 coefficients.
    pub probe: [f32; 27],
    pub use_radiance_probe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneReflections {
    /// Unity's `DefaultReflectionMode`: 0 skybox, 1 custom.
    pub mode: i32,
    pub resolution: i32,
    pub bounces: i32,
    pub intensity: f32,
    pub custom: PPtr,
    /// The reflection cubemap baked from the skybox, or null.
    pub generated_skybox: PPtr,
}

/// Legacy halo and lens flare settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HaloAndFlare {
    pub halo_strength: f32,
    pub flare_strength: f32,
    pub flare_fade_speed: f32,
    pub halo_texture: PPtr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSettings {
    pub fog: SceneFog,
    pub ambient: AmbientLight,
    /// The color of real-time shadows in Subtractive lighting mode.
    pub subtractive_shadow_color: [f32; 4],
    pub skybox_material: PPtr,
    pub halo_and_flare: HaloAndFlare,
    /// Unity's `m_SpotCookie` texture.
    pub spot_cookie: PPtr,
    pub reflections: SceneReflections,
    /// The light set as the scene's sun, or null.
    pub sun: PPtr,
    pub indirect_specular_color: [f32; 4],
}

impl Layout for RenderSettings {
    const CLASS: ClassId = ClassId::RENDER_SETTINGS;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let fog_enabled = r.bool()?;
        r.align()?;
        let fog = SceneFog {
            enabled: fog_enabled,
            color: r.floats()?,
            mode: r.i32()?,
            density: r.f32()?,
            linear_start: r.f32()?,
            linear_end: r.f32()?,
        };
        let [sky_color, equator_color, ground_color] = [r.floats()?, r.floats()?, r.floats()?];
        let ambient_intensity = r.f32()?;
        let ambient_mode = r.i32()?;
        let subtractive_shadow_color = r.floats()?;
        let skybox_material = r.pptr()?;
        let halo_and_flare = HaloAndFlare {
            halo_strength: r.f32()?,
            flare_strength: r.f32()?,
            flare_fade_speed: r.f32()?,
            halo_texture: r.pptr()?,
        };
        let spot_cookie = r.pptr()?;
        let [mode, resolution, bounces] = [r.i32()?, r.i32()?, r.i32()?];
        let intensity = r.f32()?;
        let custom = r.pptr()?;
        let probe = r.floats()?;
        let reflections = SceneReflections {
            mode,
            resolution,
            bounces,
            intensity,
            custom,
            generated_skybox: r.pptr()?,
        };
        let sun = r.pptr()?;
        let indirect_specular_color = r.floats()?;
        let ambient = AmbientLight {
            sky_color,
            equator_color,
            ground_color,
            intensity: ambient_intensity,
            mode: ambient_mode,
            probe,
            use_radiance_probe: r.bool()?,
        };
        Ok(Self {
            fog,
            ambient,
            subtractive_shadow_color,
            skybox_material,
            halo_and_flare,
            spot_cookie,
            reflections,
            sun,
            indirect_specular_color,
        })
    }
}
