//! Opt-in cascaded directional shadows for generic environment geometry.
//!
//! A retained [`ShadowedScene`] uploads geometry once and submits cascade depth passes
//! followed by a lit pass. Depth fitting is opt-in and adds GPU readback according
//! to its adoption policy; the default performs no readback. Alpha masks,
//! double-sided materials, instancing and camera-facing billboards use the same
//! geometry/material code in both passes. Terrain casts and receives through the
//! same scheduler, including planar water receivers.
//!
//! ```no_run
//! use worldrunner_render_scene::{device::GpuContext, offscreen::OffscreenTarget, shadows::*};
//! # fn frame(context: &GpuContext, scene: &worldrunner_render_plan::RenderScene,
//! #          view: &worldrunner_render_scene::SceneView) -> Result<(), ShadowError> {
//! let settings = DirectionalShadowSettings {
//!     focus_meters: [0.0, 0.0, 0.0].into(),
//!     half_extent_meters: 4.0,
//!     depth_half_extent_meters: 4.0,
//!     resolution: 1024,
//!     depth_bias: 0.001,
//!     normal_bias_meters: 0.01,
//!         refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
//! };
//! let image = capture_scene_shadowed(context, scene, view, settings)?;
//!
//! // Retain these for subsequent views/lighting changes, with no reupload. The host owns the
//! // frame renderer the scene encodes through.
//! let upload = worldrunner_render_scene::draw::PreparedScene::new(scene)?;
//! let receivers = ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1);
//! let mut renderer = ShadowedScene::new(context, upload, settings, receivers)?;
//! let mut scheduler = worldrunner_render_scene::compositor::FrameRenderer::new(context.device());
//! let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
//! let prepared = context.submission().prepare(context.device(), |encoder, _| {
//!     let attachments = (&target.color_attachment(), &target.depth_attachment());
//!     renderer.encode_with(context, view, attachments, encoder, &mut scheduler)?;
//!     target.encode_readback(encoder)?;
//!     Ok::<_, ShadowError>(())
//! }).map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
//! let image = target.read_encoded(context, prepared.submit()?)?;
//! # Ok(())
//! # }
//! ```

mod capture;
mod frame;
mod lent;
pub(crate) use capture::capture_sources;
mod cascades;
mod refresh;
mod resources;
pub(crate) use resources::SHADOW_UNIFORM_BYTES;
#[cfg(test)]
pub(crate) use resources::depth_source as caster_source;
#[cfg(test)]
pub(crate) use resources::{depth_source, receiver_sources};

/// The object shadow casters of `pipelines` for `settings` and `sampling`, for the captures that
/// draw casters outside a shadowed scene.
#[cfg(test)]
pub(crate) fn object_casters(
    pipelines: &crate::renderer::ScenePipelines,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> crate::object::program::Context {
    resources::object_caster(pipelines, settings, sampling)
}
mod settings;
mod terrain_caster;
#[cfg(test)]
pub(crate) use terrain_caster::TerrainCaster;

pub mod sdsm;
pub(crate) fn water_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    resources::layout(device)
}

pub use worldrunner_render_shadow::CascadeRefreshPolicy;

pub use settings::{DirectionalShadowSettings, MAX_SHADOW_RESOLUTION};

use worldrunner_render_plan::{RenderScene, SceneEffects};
use worldrunner_render_post::{AttachmentView, BorrowedFeature};

use crate::compositor::ColorDepth;
use crate::device::GpuContext;
use crate::draw::{DrawError, SceneGpu};
use crate::frame::{FrameError, FrameUniform};
use crate::offscreen::{OffscreenError, OffscreenTarget, RgbaImage};
use crate::renderer::ScenePipelines;
use crate::view::SceneView;
use resources::ShadowMap;

/// An opt-in shadow request could not be prepared.
#[derive(Debug, thiserror::Error)]
pub enum ShadowError {
    #[error(transparent)]
    Submission(#[from] worldrunner_render_gpu::submission::SubmitError),
    #[error(transparent)]
    DepthFitting(#[from] sdsm::SdsmError),
    /// Settings must describe a finite, representable coverage box and biases.
    #[error("invalid directional shadow fit or bias settings")]
    InvalidSettings,
    #[error(transparent)]
    Post(#[from] worldrunner_render_post::PostError),
    #[error(transparent)]
    Schedule(#[from] worldrunner_render_post::FrameError),
    /// Resolution exceeds the explicit resource ceiling or device limit.
    #[error("shadow resolution {requested} is outside 1..={limit}")]
    Resolution {
        /// Requested square texture dimension.
        requested: u32,
        /// Effective maximum dimension.
        limit: u32,
    },
    /// Shared GPU resource validation rejected the allocation.
    #[error(transparent)]
    DepthMap(#[from] worldrunner_render_shadow::DepthMapError),
    /// The bounded stock shader supports up to four cascades.
    #[error("directional shadows support one to four cascades")]
    UnsupportedCascades,
    /// A scene feature does not yet participate in the opt-in shadow passes.
    #[error("directional shadows do not support {0}")]
    UnsupportedFeature(&'static str),
    /// Scene upload failed.
    #[error(transparent)]
    Draw(#[from] DrawError),
    /// Frame lighting could not be derived.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// Target allocation or readback failed.
    #[error(transparent)]
    Offscreen(#[from] OffscreenError),
}

/// The colour attachments a [`ShadowedScene`]'s receivers draw into, and the retained
/// generation whose unchanged streamed chunk buffers it shares.
#[derive(Clone, Copy, Debug)]
pub struct ShadowedTarget<'a> {
    /// Colour attachment format of the receivers.
    pub format: wgpu::TextureFormat,
    /// Sample count of the colour and depth attachments.
    pub samples: u32,
    /// Whether the receivers also write the object-motion residual for a frame renderer with
    /// TAA resident, as [`ScenePipelines::with_taa_velocity`] does.
    pub velocity: bool,
    /// The preceding retained generation, or `None` for a scene uploaded whole.
    pub previous: Option<&'a SceneGpu>,
}

impl ShadowedTarget<'_> {
    /// Receivers drawing into `format` with `samples` samples, writing no velocity and sharing
    /// nothing with an earlier generation.
    #[must_use]
    pub const fn new(format: wgpu::TextureFormat, samples: u32) -> Self {
        Self {
            format,
            samples,
            velocity: false,
            previous: None,
        }
    }
}

/// Reusable resident scene, pipelines and a bounded directional depth array.
///
/// Attachment views must belong to the supplied context, match the view's
/// viewport and the receiver pipelines' colour/sample contract (its [`ShadowedTarget`]),
/// with `Depth32Float` depth. This is also usable without an offscreen target by a compatible
/// viewer.
#[derive(Debug)]
pub struct ShadowedScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    /// The object casters on the program runtime the receivers' objects draw with.
    object_caster: crate::object::program::Context,
    terrain_caster: terrain_caster::TerrainCaster,
    caster_slope_bits: u32,
    depth_frames: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    depth_slots: Vec<Vec<(wgpu::Buffer, wgpu::BindGroup)>>,
    frame_slot: usize,
    shadow_layout: wgpu::BindGroupLayout,
    map: ShadowMap,
    settings: DirectionalShadowSettings,
    shadows_enabled: bool,
    refresh: std::rc::Rc<std::cell::RefCell<refresh::RefreshCache>>,
    depth_fitting: Option<sdsm::scene::SceneDepthFitting>,
    effects: SceneEffects,
}

impl ShadowedScene {
    /// Select a live admission's slot; reuse only after that submission completes.
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.frame_slot = slot;
        self.gpu.select_frame_slot(slot);
        self.depth_frames = self.depth_slots[slot].clone();
        self.map.select_frame_slot(slot);
    }
    /// Validates the opt-in contract, then builds receiver pipelines for `target` and uploads
    /// `prepared`, sharing unchanged streamed chunk buffers with `target.previous`.
    pub fn new(
        context: &GpuContext,
        prepared: crate::draw::PreparedScene,
        settings: DirectionalShadowSettings,
        target: ShadowedTarget<'_>,
    ) -> Result<Self, ShadowError> {
        let ShadowedTarget {
            format,
            samples,
            velocity,
            previous,
        } = target;
        let scene = prepared.scene().clone();
        let scene = &scene;
        worldrunner_render_post::FrameConfig::new([1, 1], format, samples)
            .validate_adapter(context.adapter())?;
        settings.validate(&context.device().limits())?;
        settings::validate_scene(scene)?;
        settings.validate_light(scene.effects().ok_or(ShadowError::UnsupportedFeature(
            "shadows require an environmental profile",
        ))?)?;
        let effects = scene.effects().expect("validated effects");
        let sampling = effects.shadows().sampling();
        let shadow_layout = resources::layout(context.device());
        let pipelines = resources::receiver_pipelines(
            context,
            scene,
            (format, samples, velocity),
            &shadow_layout,
        )?;
        let object_caster = resources::object_caster(&pipelines, settings, sampling);
        let terrain_caster = resources::terrain_caster(context, &pipelines, settings, sampling);
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            prepared,
            &pipelines,
            previous,
        )?;
        let depth_frames = resources::depth_frames(context, pipelines.frame_layout());
        let layers =
            u32::try_from(effects.shadows().extents_millimeters().len()).expect("four cascades");
        let map = ShadowMap::new(context, settings, &shadow_layout, layers)?;
        let depth_slots = vec![
            depth_frames.clone(),
            resources::depth_frames(context, pipelines.frame_layout()),
            resources::depth_frames(context, pipelines.frame_layout()),
        ];
        Ok(Self {
            gpu,
            pipelines,
            object_caster,
            terrain_caster,
            caster_slope_bits: resources::slope_bias(sampling).to_bits(),
            depth_slots,
            depth_frames,
            frame_slot: 0,
            depth_fitting: None,
            refresh: std::rc::Rc::default(),
            shadow_layout,
            map,
            settings,
            shadows_enabled: true,
            effects: effects.clone(),
        })
    }

    /// Owned counts from the most recently submitted main-camera pass.
    pub fn view_stats(&self) -> crate::draw::ViewStats {
        self.gpu.view_stats()
    }

    /// Enables directional shadows for subsequent encodes. Defaults to enabled.
    /// Disabled frames skip caster passes and receiver sampling, retaining cached maps.
    pub fn set_shadows_enabled(&mut self, enabled: bool) {
        self.shadows_enabled = enabled;
    }

    /// Reports whether directional shadows are enabled for this resident scene.
    pub fn shadows_enabled(&self) -> bool {
        self.shadows_enabled
    }

    /// Owned counts from the most recently submitted shadow cascade zero.
    pub fn shadow_view_stats(&self) -> crate::draw::ViewStats {
        self.gpu.shadow_view_stats()
    }

    /// Owned counts for a particular submitted cascade.
    pub fn cascade_view_stats(&self, index: usize) -> crate::draw::ViewStats {
        self.gpu.cascade_view_stats(index)
    }

    /// Returns the resident scene for handle-based placement edits.
    pub fn scene_mut(&mut self) -> &mut SceneGpu {
        &mut self.gpu
    }

    /// The resident scene with the pipelines its material bind groups are built against, for
    /// appending resources.
    pub fn scene_and_pipelines_mut(&mut self) -> (&mut SceneGpu, &ScenePipelines) {
        (&mut self.gpu, &self.pipelines)
    }

    /// Updates fit/bias, recreating only the map if its resolution changes.
    /// A rejected update leaves the current settings and resources intact.
    pub fn set_settings(
        &mut self,
        context: &GpuContext,
        settings: DirectionalShadowSettings,
    ) -> Result<(), ShadowError> {
        settings.validate(&context.device().limits())?;
        settings.validate_light(&self.effects)?;
        if settings.resolution != self.settings.resolution {
            self.map = ShadowMap::new(
                context,
                settings,
                &self.shadow_layout,
                self.map.depth.layers(),
            )?;
        }
        if self.settings != settings {
            self.invalidate_shadows();
        }
        self.settings = settings;
        Ok(())
    }

    /// Changes lighting and cascade policy without uploading geometry again.
    /// Changed layer counts are allocated at the next ordinary encode. When depth
    /// fitting is enabled, rendering instead returns [`sdsm::SdsmError::CascadeLayers`]
    /// before encoding. Rebuild the renderer or disable fitting to change layer counts.
    pub fn set_effects(&mut self, effects: &SceneEffects) -> Result<(), ShadowError> {
        settings::validate_effects(effects)?;
        if effects.ssao().is_some() != self.pipelines.ssao {
            return Err(FrameError::UnsupportedEffect("SSAO").into());
        }
        self.settings.validate_light(effects)?;
        if self.effects.ssao() != effects.ssao() {
            return Err(worldrunner_render_post::FrameError::SsaoSettings(
                "SSAO settings are immutable; rebuild the frame profile and resident scene",
            )
            .into());
        }
        if self.effects.water_bodies() != effects.water_bodies() {
            return Err(worldrunner_render_post::FrameError::WaterSettings(
                "water settings are immutable; rebuild the profile and resident",
            )
            .into());
        }
        self.gpu.set_ssr_settings(effects.ssr().copied())?;
        if self.effects.shadows() != effects.shadows() {
            self.invalidate_shadows();
        }
        self.effects = effects.clone();
        Ok(())
    }

    fn validate_scheduler(
        scheduler: &crate::compositor::FrameRenderer,
        pipelines: &ScenePipelines,
        effects: &SceneEffects,
    ) -> Result<(), ShadowError> {
        scheduler.validate_ssao(pipelines, effects.ssao())?;
        scheduler.validate_water(effects.water_bodies())?;
        scheduler.validate_ssr(effects.ssr())?;
        scheduler.validate_post(
            crate::post::settings(effects)
                .map_err(|_| FrameError::UnsupportedEffect("post settings"))?,
        )?;
        Ok(())
    }

    /// Encodes all uploads and passes without submitting, for composition with output.
    /// The host's retained scheduler serves both scene and output composition.
    /// Requires the active transaction encoder. Resident publication happens on
    /// submission; dropping the prepared frame discards its publications.
    /// Checked attachments retain their selected mip and array layer.
    pub fn encode_with(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<(), ShadowError> {
        let attachments = ColorDepth {
            color: attachments.0,
            depth: attachments.1,
        };
        let input = frame::FitInput {
            scheduler,
            fitting: None,
        };
        self.encode_fitted(context, view, attachments, encoder, input)
    }

    /// Admitted player program draws are drawn by the configured compositor only. Admitted
    /// offscreen operations are taken by the frame and run before its first pass.
    fn refuse_programs(&self) -> Result<(), ShadowError> {
        if self.gpu.has_programs() {
            return Err(ShadowError::UnsupportedFeature("player programs"));
        }
        Ok(())
    }

    fn prepare_cascades(
        scene: (&SceneGpu, &SceneEffects, DirectionalShadowSettings),
        view: &SceneView,
        sun: glam::Vec3,
        fitting: Option<(sdsm::DepthRange, sdsm::RangeFit)>,
    ) -> Result<cascades::CascadeViews, ShadowError> {
        let (gpu, effects, settings) = scene;
        match fitting {
            Some(fitting) => {
                cascades::CascadeViews::fitted((gpu, effects), view, settings, sun, fitting)
            }
            None => cascades::CascadeViews::new(gpu, view, effects, settings, sun),
        }
    }

    fn update_caster_pipelines(&mut self, context: &GpuContext) {
        let sampling = self.effects.shadows().sampling();
        let bits = resources::slope_bias(sampling).to_bits();
        if bits != self.caster_slope_bits {
            self.object_caster = resources::object_caster(&self.pipelines, self.settings, sampling);
            self.terrain_caster =
                resources::terrain_caster(context, &self.pipelines, self.settings, sampling);
            self.caster_slope_bits = bits;
        }
    }

    /// Prepares the terrain for every pass of the frame before any opens: the prepass, the
    /// forward pass, and the casters of the cascades, and the objects of the prepass, the forward
    /// pass and, when the frame has shadow passes, the cascades.
    fn prepare_terrain_passes(
        (gpu, caster, objects): (
            &SceneGpu,
            &terrain_caster::TerrainCaster,
            Option<&crate::object::program::Context>,
        ),
        (screen, selected, frame): (
            Option<&crate::scene_metadata::Geometry>,
            &ScenePipelines,
            &FrameUniform,
        ),
    ) -> Result<(), ShadowError> {
        if let Some(screen) = screen {
            gpu.prepare_terrain(screen.prepass_pipelines(), frame)?;
            gpu.prepare_objects(screen.prepass_pipelines(), frame)?;
        }
        gpu.prepare_terrain(selected, frame)?;
        gpu.prepare_objects(selected, frame)?;
        gpu.prepare_celestial(frame)?;
        // A frame without shadow passes draws no object casters.
        if let Some(objects) = objects {
            gpu.prepare_object_context(objects)?;
        }
        caster.pipeline().map_err(|error| {
            worldrunner_render_post::FrameError::FeaturePreparation {
                feature: "terrain shadow caster",
                reason: error.to_string(),
            }
        })?;
        Ok(())
    }

    fn replacement_map(&self, context: &GpuContext) -> Result<Option<ShadowMap>, ShadowError> {
        let count = u32::try_from(self.effects.shadows().extents_millimeters().len())
            .expect("four cascades");
        if self.map.depth.layers() == count {
            return Ok(None);
        }
        Ok(Some(ShadowMap::new(
            context,
            self.settings,
            &self.shadow_layout,
            count,
        )?))
    }
}

/// Uploads, shadow-renders and reads a scene, without encoding or filesystem I/O.
/// For repeated frames retain [`ShadowedScene`] and [`OffscreenTarget`] instead.
pub fn capture_scene_shadowed(
    context: &GpuContext,
    scene: &RenderScene,
    view: &SceneView,
    settings: DirectionalShadowSettings,
) -> Result<RgbaImage, ShadowError> {
    settings.validate(&context.device().limits())?;
    settings::validate_scene(scene)?;
    settings.validate_light(scene.effects().ok_or(ShadowError::UnsupportedFeature(
        "shadows require an environmental profile",
    ))?)?;
    let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
    let mut renderer = ShadowedScene::new(
        context,
        crate::draw::PreparedScene::new(scene)?,
        settings,
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )?;
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_with(
                context,
                view,
                (&target.color_attachment(), &target.depth_attachment()),
                encoder,
                &mut scheduler,
            )?;
            target.encode_readback(encoder)?;
            Ok::<_, ShadowError>(())
        })
        .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
    let submission = prepared.submit()?;
    Ok(target.read_encoded(context, submission)?)
}

#[cfg(test)]
mod tests;

impl From<std::convert::Infallible> for ShadowError {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}

/// The uniform a shadow caster pass draws with: the light's own matrix and nothing of the
/// viewer's sub-pixel jitter, which would make cascade edges move with the camera. The material
/// mip bias is a screen-space policy for the reconstructed view, so a light-view raster uses 0.
pub(crate) fn light_frame(frame: &FrameUniform, matrix: glam::Mat4) -> FrameUniform {
    FrameUniform {
        // Directional light fits use right-handed views independently of the viewer.
        view_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        view_projection: matrix,
        temporal: None,
        material_mip_bias: 0.0,
        ..*frame
    }
}

#[cfg(test)]
pub(crate) fn wind_test_pipelines(
    device: &wgpu::Device,
    layouts: &ScenePipelines,
) -> [wgpu::RenderPipeline; crate::pipeline::PipelineKey::ALL.len()] {
    resources::depth_pipelines(
        device,
        layouts,
        DirectionalShadowSettings {
            focus_meters: glam::Vec3::ZERO,
            half_extent_meters: 4.0,
            depth_half_extent_meters: 4.0,
            resolution: 64,
            depth_bias: 0.0,
            normal_bias_meters: 0.0,
            refresh_policy: CascadeRefreshPolicy::default(),
        },
        worldrunner_render_plan::SceneShadowSampling::default(),
    )
}
