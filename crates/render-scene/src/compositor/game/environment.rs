//! The stock specular environment cube a game's passes sample: the cube the renderer's own
//! surface reflections (or, without them, water) sample, with the scale and roughness
//! convention they sample it by.
use super::lent::{FrameInputError, Lent};
use worldrunner_render_plan::SceneSpecularConvention;

/// Which stock consumer's installed cube a [`FrameEnvironment`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentConsumer {
    /// The surface reflections (screen-space reflection fallback and forward reflection).
    Reflections,
    /// Water, lent only when no reflection cube is installed, and whether or not the stock
    /// passes draw the water.
    Water,
}

/// The specular environment cube the frame in flight samples, as the stock consumer samples it.
///
/// It is the cube the host installed from the scene's environment
/// (`SceneGpu::environment_maps`): when the scene's `SceneSpecularCube` requests preparation,
/// the engine's GGX prefiltered cube (`Rgba16Float`); otherwise the uploaded mips as authored.
/// The filter stores, at level `m` of `n`, the radiance prefiltered for roughness
/// [`Self::level_roughness`]`(m) = m / (n - 1)` (before the convention's roughness remap, as
/// the filter applies it) with GGX alpha the square of the remapped roughness; the texels are in
/// the source cube's radiance units, with no scale applied. The stock consumers sample level
/// [`Self::level`]`(r) = r * (n - 1)` for remapped roughness `r`, trilinearly, and multiply the
/// result by [`Self::radiance_scale`]. Under the perceptual conventions, whose remap is the
/// identity, the level sampled for a roughness is the level filtered for it.
#[derive(Clone, Debug)]
pub struct FrameEnvironment {
    /// The cube: a square two-dimensional texture with six layers and [`Self::mip_count`] levels.
    pub texture: wgpu::Texture,
    /// The whole texture as a cube view, every level.
    pub cube: wgpu::TextureView,
    /// The levels of [`Self::texture`].
    pub mip_count: u32,
    /// The stock consumer's multiplier on sampled radiance (the environment's specular
    /// intensity). It is not baked into the texels.
    pub radiance_scale: f32,
    /// The roughness remap and specular visibility the cube and the stock BRDF table follow.
    pub convention: SceneSpecularConvention,
    /// Which installed consumer the cube is.
    pub consumer: EnvironmentConsumer,
}

impl FrameEnvironment {
    /// The level the stock consumers sample for remapped roughness `roughness`.
    #[expect(clippy::cast_precision_loss, reason = "at most a few dozen mip levels")]
    pub fn level(&self, roughness: f32) -> f32 {
        roughness * (self.mip_count - 1) as f32
    }

    /// The roughness the engine's filter prefilters level `mip` for, before the convention's
    /// remap. Meaningful only for a cube the engine filtered.
    pub fn level_roughness(&self, mip: u32) -> f32 {
        crate::sky::preparation::level_roughness(mip, self.mip_count)
    }
}

/// Where a game reads the stock specular environment of the frame in flight.
///
/// Every route publishes it as it begins encoding the frame, before any feature plans. The cube
/// is the renderer's own retained texture: sample it, never write it. It changes only when the
/// host installs or clears an environment, but a feature reads it each frame like the other
/// lent inputs.
#[derive(Clone, Debug, Default)]
pub struct FrameEnvironmentHandle(Lent<Option<FrameEnvironment>>);

impl FrameEnvironmentHandle {
    /// The environment of the frame now being prepared; `None` when the host installed no
    /// environment cube for the reflections or water. A water cube is lent even while a game
    /// that declares the water keeps the stock passes from drawing it.
    ///
    /// # Errors
    /// [`FrameInputError::OutsideFrame`] outside a frame, and [`FrameInputError::NotPrepared`]
    /// before the frame has begun encoding.
    pub fn current(&self) -> Result<Option<FrameEnvironment>, FrameInputError> {
        self.0.current()
    }

    pub(crate) fn publish(&self, environment: Option<FrameEnvironment>) {
        self.0.publish(environment);
    }
}

impl crate::compositor::FrameRenderer {
    /// The installed cube of the reflections, else of water; `None` without either.
    ///
    /// The water cube is lent from the installed water, not from the water the stock passes
    /// draw: a game that suppresses the stock water draws still installed that water's
    /// environment, for its own water passes to sample.
    pub(crate) fn lend_environment(&self) -> Option<FrameEnvironment> {
        let [reflections, water] = self.environment_conventions;
        let (views, intensity_micro, convention, consumer) =
            if let (Some(convention), Some(ssr)) = (reflections, self.ssr.as_ref()) {
                (
                    &ssr.environment,
                    ssr.environment_intensity_micro,
                    convention,
                    EnvironmentConsumer::Reflections,
                )
            } else if let (Some(convention), Some(water)) = (water, self.water.as_ref()) {
                (
                    &water.environment,
                    water.environment_intensity_micro,
                    convention,
                    EnvironmentConsumer::Water,
                )
            } else {
                return None;
            };
        let cube = views[0].clone();
        let texture = cube.texture().clone();
        #[expect(clippy::cast_precision_loss, reason = "intensity is at most 64e6")]
        let radiance_scale = intensity_micro as f32 / 1e6;
        Some(FrameEnvironment {
            mip_count: texture.mip_level_count(),
            texture,
            cube,
            radiance_scale,
            convention,
            consumer,
        })
    }
}
