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
//! // Retain these for subsequent views/lighting changes, with no reupload.
//! let mut renderer = ShadowedScene::new(context, scene, settings)?;
//! let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
//! renderer.render(context, view, &target.color_attachment(), &target.depth_attachment())?;
//! let image = target.read_rgba8(context)?;
//! # Ok(())
//! # }
//! ```

mod cascades;
mod refresh;
mod resources;
mod schedule;
mod settings;

pub mod sdsm;
pub(crate) fn water_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    resources::layout(device)
}

pub use worldrunner_render_shadow::CascadeRefreshPolicy;

pub use settings::{DirectionalShadowSettings, MAX_SHADOW_RESOLUTION};

use worldrunner_render_plan::{RenderScene, SceneEffects};
use worldrunner_render_post::{AttachmentView, BorrowedFeature};

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

/// Reusable resident scene, pipelines and a bounded directional depth array.
///
/// Attachment views must belong to the supplied context, match the view's
/// viewport and the receiver pipelines' colour/sample contract, with `Depth32Float` depth.
/// `new` selects single-sample `Rgba8UnormSrgb`; `with_target` selects an explicit target.
/// This is also usable without an offscreen target by a compatible viewer.
#[derive(Debug)]
pub struct ShadowedScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    depth_pipelines: [wgpu::RenderPipeline; 4],
    terrain_depth_pipeline: wgpu::RenderPipeline,
    depth_frames: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    depth_slots: Vec<Vec<(wgpu::Buffer, wgpu::BindGroup)>>,
    frame_slot: usize,
    shadow_layout: wgpu::BindGroupLayout,
    map: ShadowMap,
    settings: DirectionalShadowSettings,
    refresh: std::rc::Rc<std::cell::RefCell<refresh::RefreshCache>>,
    depth_fitting: Option<sdsm::scene::SceneDepthFitting>,
    effects: SceneEffects,
    scheduler: crate::compositor::FrameRenderer,
}

impl ShadowedScene {
    /// Select a live admission's slot; reuse only after that submission completes.
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.frame_slot = slot;
        self.scheduler.select_frame_slot(slot);
        self.gpu.select_frame_slot(slot);
        self.depth_frames = self.depth_slots[slot].clone();
        self.map.select_frame_slot(slot);
    }
    /// Validates the opt-in contract before allocating or uploading scene resources.
    pub fn new(
        context: &GpuContext,
        scene: &RenderScene,
        settings: DirectionalShadowSettings,
    ) -> Result<Self, ShadowError> {
        Self::with_target(context, scene, settings, OffscreenTarget::COLOR_FORMAT, 1)
    }

    /// Builds receiver pipelines for a validated frame attachment profile.
    pub fn with_target(
        context: &GpuContext,
        scene: &RenderScene,
        settings: DirectionalShadowSettings,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Result<Self, ShadowError> {
        worldrunner_render_post::FrameConfig::new([1, 1], format, samples)
            .validate_adapter(context.adapter())?;
        settings.validate(&context.device().limits())?;
        settings::validate_scene(scene)?;
        settings.validate_light(scene.effects().ok_or(ShadowError::UnsupportedFeature(
            "shadows require an environmental profile",
        ))?)?;
        let shadow_layout = resources::layout(context.device());
        let pipelines =
            resources::receiver_pipelines(context, scene, (format, samples), &shadow_layout)?;
        let (depth_pipelines, terrain_depth_pipeline) =
            resources::depth_pipelines(context.device(), &pipelines, settings);
        let gpu = SceneGpu::upload(context.device(), context.submission(), scene, &pipelines)?;
        let depth_frames = resources::depth_frames(context, pipelines.frame_layout());
        let layers = u32::try_from(
            scene
                .effects()
                .expect("validated effects")
                .shadows()
                .extents_millimeters()
                .len(),
        )
        .expect("four cascades");
        let map = ShadowMap::new(context, settings, &shadow_layout, layers)?;
        let depth_slots = vec![
            depth_frames.clone(),
            resources::depth_frames(context, pipelines.frame_layout()),
            resources::depth_frames(context, pipelines.frame_layout()),
        ];
        Ok(Self {
            scheduler: crate::compositor::FrameRenderer::new(context.device()),
            gpu,
            pipelines,
            depth_pipelines,
            terrain_depth_pipeline,
            depth_slots,
            depth_frames,
            frame_slot: 0,
            depth_fitting: None,
            refresh: std::rc::Rc::default(),
            shadow_layout,
            map,
            settings,
            effects: scene
                .effects()
                .cloned()
                .ok_or(ShadowError::UnsupportedFeature(
                    "shadows require an environmental profile",
                ))?,
        })
    }

    /// Owned counts from the most recently submitted main-camera pass.
    pub fn view_stats(&self) -> crate::draw::ViewStats {
        self.gpu.view_stats()
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
        if self.effects.water() != effects.water() {
            return Err(worldrunner_render_post::FrameError::WaterSettings(
                "water settings are immutable; rebuild the profile and resident",
            )
            .into());
        }
        if self.effects.ssr() != effects.ssr() {
            return Err(worldrunner_render_post::FrameError::SsrSettings(
                "settings are immutable; rebuild the profile and resident",
            )
            .into());
        }
        if self.effects.shadows() != effects.shadows() {
            self.invalidate_shadows();
        }
        self.effects = effects.clone();
        Ok(())
    }

    /// Submits cascade depth and colour passes, clearing colour to black and depth to one.
    /// Each view reuses its host-side visible-range storage after warmup.
    /// Without depth fitting no readback occurs. Interactive fitting polls readback
    /// without waiting; same-frame capture waits for metadata reduction before
    /// submitting fitted shadows and colour. Use the same device as construction.
    pub fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        color: &AttachmentView,
        depth: &AttachmentView,
    ) -> Result<(), ShadowError> {
        if let Some(fitting) = &self.depth_fitting {
            let frame = fitting.next_frame;
            self.render_depth_fitted(context, view, (color, depth), frame)?;
            return Ok(());
        }
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                self.encode(context, view, color, depth, encoder)
            })
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        prepared.submit()?;
        Ok(())
    }

    /// Encodes all uploads and passes without submitting, for composition with output.
    /// Requires the active transaction encoder. Resident publication happens on
    /// submission; dropping the prepared frame discards its publications.
    /// Checked attachments retain their selected mip and array layer.
    pub fn encode(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        color: &AttachmentView,
        depth: &AttachmentView,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ShadowError> {
        self.encode_with(context, view, (color, depth), encoder, None)
    }

    fn validate_scheduler(
        scheduler: &crate::compositor::FrameRenderer,
        pipelines: &ScenePipelines,
        effects: &SceneEffects,
    ) -> Result<(), ShadowError> {
        scheduler.validate_ssao(pipelines, effects.ssao())?;
        scheduler.validate_water(effects.water())?;
        scheduler.validate_ssr(effects.ssr())?;
        scheduler.validate_post(
            crate::post::settings(effects)
                .map_err(|_| FrameError::UnsupportedEffect("post settings"))?,
        )?;
        Ok(())
    }

    /// Uses a host's retained scheduler for both scene and output composition.
    /// The submission, abandonment and checked-attachment rules of [`Self::encode`] apply.
    pub fn encode_with(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scheduler: Option<&mut crate::compositor::FrameRenderer>,
    ) -> Result<(), ShadowError> {
        self.encode_fitted(context, view, attachments, encoder, (scheduler, None))
    }

    /// Encode with completed depth evidence. Empty depth clears all maps and disables
    /// receiver sampling. The ordinary encode path retains its original schedule.
    pub fn encode_depth_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        fitting: (sdsm::AdoptedFit, sdsm::RangeFit),
    ) -> Result<(), ShadowError> {
        self.encode_fitted(
            context,
            view,
            attachments,
            encoder,
            (None, Some((fitting.0.depth, fitting.1))),
        )
    }

    fn prepare_cascades(
        scene: (&SceneGpu, &SceneEffects, DirectionalShadowSettings),
        view: &SceneView,
        sun: glam::Vec3,
        fitting: Option<(sdsm::DepthRange, sdsm::RangeFit)>,
    ) -> Result<cascades::CascadeViews, ShadowError> {
        let (gpu, effects, settings) = scene;
        let mut cascades = cascades::CascadeViews::new(gpu, view, effects, settings, sun)?;
        if let Some(fitting) = fitting {
            cascades.refit_depth(gpu, view, settings, sun, fitting)?;
        }
        Ok(cascades)
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

    fn encode_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        input: (
            Option<&mut crate::compositor::FrameRenderer>,
            Option<(sdsm::DepthRange, sdsm::RangeFit)>,
        ),
    ) -> Result<(), ShadowError> {
        let mut replacement = self.replacement_map(context)?;
        let (scheduler, fitting) = input;
        let (color, depth) = attachments;
        let scheduler = scheduler.unwrap_or(&mut self.scheduler);
        Self::validate_scheduler(scheduler, &self.pipelines, &self.effects)?;
        let frame = scheduler.prepare_uniform(FrameUniform::new(view, Some(&self.effects))?)?;
        self.gpu
            .prepare_terrain_pages(context.device(), encoder, frame, color.size())?;
        let mut cascades = Self::prepare_cascades(
            (&self.gpu, &self.effects, self.settings),
            view,
            frame.sun_direction,
            fitting,
        )?;
        let mut next_refresh = self.refresh.borrow().clone();
        if replacement.is_some() {
            next_refresh.invalidate();
        }
        let refresh = next_refresh.prepare(
            &mut cascades,
            self.settings,
            frame.sun_direction,
            color.size(),
            self.gpu.caster_generation(),
        );
        let map = replacement.as_mut().unwrap_or(&mut self.map);
        map.select_frame_slot(self.frame_slot);
        let mut schedule =
            schedule::ShadowSchedule::new(&map.depth, color, depth, &self.pipelines, refresh)?;
        schedule.screen_space(scheduler);
        schedule.celestial(&self.gpu, context.device(), color.size());
        let mut feature = ShadowFeature {
            publications: crate::draw::DrawPublications::default(),
            screen: scheduler.ssao.as_ref().map(|ssao| ssao.geometry.clone()),
            gpu: &mut self.gpu,
            pipelines: &self.pipelines,
            depth_pipelines: &self.depth_pipelines,
            terrain_depth_pipeline: &self.terrain_depth_pipeline,
            depth_frames: &self.depth_frames,
            shadow_group: &map.group,
            cascades,
            frame,
            shadow_uniform: &map.uniform,
            settings: self.settings,
        };
        scheduler.encode_frame(
            context.device(),
            encoder,
            (&schedule.passes, &schedule.views),
            &mut [BorrowedFeature {
                declaration: schedule.declaration,
                prepared: &mut feature,
            }],
            (frame, Some(&map.group)),
        )?;
        if let Some(map) = replacement {
            self.map = map;
        }
        next_refresh.publish(encoder, &self.refresh)?;
        Ok(())
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
    let mut renderer = ShadowedScene::new(context, scene, settings)?;
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode(
                context,
                view,
                &target.color_attachment(),
                &target.depth_attachment(),
                encoder,
            )?;
            target.encode_readback(encoder)?;
            Ok::<_, ShadowError>(())
        })
        .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
    let submission = prepared.submit()?;
    Ok(target.read_encoded(context, submission)?)
}

struct ShadowFeature<'a> {
    publications: crate::draw::DrawPublications,
    screen: Option<crate::scene_metadata::Geometry>,
    gpu: &'a mut SceneGpu,
    pipelines: &'a ScenePipelines,
    depth_pipelines: &'a [wgpu::RenderPipeline; 4],
    terrain_depth_pipeline: &'a wgpu::RenderPipeline,
    depth_frames: &'a [(wgpu::Buffer, wgpu::BindGroup)],
    shadow_group: &'a wgpu::BindGroup,
    cascades: cascades::CascadeViews,
    frame: FrameUniform,
    shadow_uniform: &'a wgpu::Buffer,
    settings: DirectionalShadowSettings,
}
impl worldrunner_render_post::PreparedFeature for ShadowFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
        // Each light view has its own immutable uniform range; billboards keep the main eye.
        for (index, &matrix) in self.cascades.matrices.iter().enumerate() {
            let light_frame = FrameUniform {
                view_projection: matrix,
                ..self.frame
            };
            context.upload_buffer(&self.depth_frames[index].0, 0, &light_frame.to_bytes());
        }
        context.upload_buffer(
            self.shadow_uniform,
            0,
            &resources::uniform_bytes(&self.cascades, self.settings),
        );
    }
    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.publications.publish(context);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if context.slot == worldrunner_render_post::PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        if context.slot == worldrunner_render_post::PassSlot::ReflectionMaterial {
            let screen = self.screen.as_ref().expect("declared metadata");
            self.publications.record(self.gpu.draw_prepass(
                pass,
                screen.reflection.as_ref().expect("reflection pipelines"),
                &screen.group,
            ));
        } else if context.slot == worldrunner_render_post::PassSlot::DepthNormalPrepass {
            let screen = self.screen.as_ref().expect("declared prepass");
            self.publications.record(
                self.gpu
                    .draw_prepass(pass, &screen.pipelines, &screen.group),
            );
        } else if let worldrunner_render_post::PassSlot::ShadowCasters(index) = context.slot {
            if self.cascades.no_receivers {
                return;
            }
            let matrix = self.cascades.matrices[index as usize];
            let group = &self.depth_frames[index as usize].1;
            self.publications.record(self.gpu.draw_objects(
                pass,
                group,
                self.depth_pipelines,
                Some((index, matrix)),
            ));
            self.gpu
                .draw_terrain_depth(pass, group, self.terrain_depth_pipeline, matrix);
        } else {
            if let Some(screen) = &self.screen {
                pass.set_bind_group(3, &screen.ambient, &[]);
            }
            pass.set_bind_group(2, self.shadow_group, &[]);
            self.publications.record(self.gpu.draw_shadowed(
                pass,
                self.pipelines,
                self.shadow_group,
            ));
        }
    }
}

#[cfg(test)]
mod tests;

impl From<std::convert::Infallible> for ShadowError {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}
