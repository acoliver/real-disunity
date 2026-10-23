//! Opt-in single-map directional shadows for generic environment geometry.
//!
//! A retained [`ShadowedScene`] uploads geometry once and submits a depth pass
//! followed by a lit pass, without readback or filesystem policy. Alpha masks,
//! double-sided materials, instancing and camera-facing billboards use the same
//! geometry/material code in both passes. Terrain, water and multiple cascades
//! are rejected rather than silently rendered without their requested shadows.
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

mod resources;
mod settings;

pub use settings::{DirectionalShadowSettings, MAX_SHADOW_RESOLUTION};

use worldrunner_render_plan::{RenderScene, SceneEffects};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, PassDeclaration, PassSlot, ResourceRef,
};

use crate::device::GpuContext;
use crate::draw::{DrawError, SceneGpu};
use crate::frame::{FRAME_UNIFORM_BYTES, FrameError, FrameUniform};
use crate::offscreen::{OffscreenError, OffscreenTarget, RgbaImage};
use crate::renderer::ScenePipelines;
use crate::upload::upload_buffer;
use crate::view::SceneView;
use resources::{SHADOWED_WGSL, ShadowMap};

/// An opt-in shadow request could not be prepared.
#[derive(Debug, thiserror::Error)]
pub enum ShadowError {
    /// Settings must describe a finite, representable coverage box and biases.
    #[error("invalid directional shadow fit or bias settings")]
    InvalidSettings,
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
    /// No cascade splitting or blending is implemented.
    #[error("directional shadows require exactly one declared cascade and zero blend band")]
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

/// Reusable resident scene, pipelines and one bounded directional shadow map.
///
/// Attachment views must belong to the supplied context, match the view's
/// viewport, use sample count 1, `Rgba8UnormSrgb` colour and `Depth32Float` depth.
/// This is also usable without an offscreen target by a compatible viewer.
#[derive(Debug)]
pub struct ShadowedScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    depth_pipelines: [wgpu::RenderPipeline; 4],
    depth_frame: wgpu::Buffer,
    depth_frame_group: wgpu::BindGroup,
    shadow_layout: wgpu::BindGroupLayout,
    map: ShadowMap,
    settings: DirectionalShadowSettings,
    effects: SceneEffects,
    scheduler: crate::compositor::FrameRenderer,
}

impl ShadowedScene {
    /// Validates the opt-in contract before allocating or uploading scene resources.
    pub fn new(
        context: &GpuContext,
        scene: &RenderScene,
        settings: DirectionalShadowSettings,
    ) -> Result<Self, ShadowError> {
        settings.validate(&context.device().limits())?;
        settings::validate_scene(scene)?;
        settings.validate_light(scene.effects().ok_or(ShadowError::UnsupportedFeature(
            "shadows require an environmental profile",
        ))?)?;
        let shadow_layout = resources::layout(context.device());
        let pipelines = ScenePipelines::with_environment(
            context.device(),
            OffscreenTarget::COLOR_FORMAT,
            SHADOWED_WGSL,
            Some(&shadow_layout),
        );
        let depth_pipelines = resources::depth_pipelines(context.device(), &pipelines, settings);
        let gpu = SceneGpu::upload(context.device(), context.queue(), scene, &pipelines)?;
        let depth_frame = upload_buffer(
            context.device(),
            context.queue(),
            "shadow frame",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
            &[0; FRAME_UNIFORM_BYTES],
        );
        let depth_frame_group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow frame"),
                layout: pipelines.frame_layout(),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: depth_frame.as_entire_binding(),
                }],
            });
        let map = ShadowMap::new(context, settings, &shadow_layout)?;
        Ok(Self {
            scheduler: crate::compositor::FrameRenderer::new(context.device()),
            gpu,
            pipelines,
            depth_pipelines,
            depth_frame,
            depth_frame_group,
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

    /// Counts from the most recently recorded main-camera pass.
    pub fn view_stats(&self) -> std::cell::Ref<'_, crate::draw::ViewStats> {
        self.gpu.view_stats()
    }

    /// Counts from the most recently recorded shadow cascade (index zero).
    pub fn shadow_view_stats(&self) -> std::cell::Ref<'_, crate::draw::ViewStats> {
        self.gpu.shadow_view_stats()
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
            self.map = ShadowMap::new(context, settings, &self.shadow_layout)?;
        }
        self.settings = settings;
        Ok(())
    }

    /// Changes lighting without uploading geometry again; the same single-cascade contract applies.
    pub fn set_effects(&mut self, effects: &SceneEffects) -> Result<(), ShadowError> {
        settings::validate_effects(effects)?;
        self.settings.validate_light(effects)?;
        self.effects = effects.clone();
        Ok(())
    }

    /// Submits both passes, clearing colour to opaque black and depth to one.
    /// Each view reuses its host-side visible-range storage after warmup.
    /// No readback occurs. Use the same device as construction.
    pub fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        color: &AttachmentView,
        depth: &AttachmentView,
    ) -> Result<(), ShadowError> {
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("shadowed scene"),
                });
        self.encode(context, view, color, depth, &mut encoder)?;
        context.queue().submit([encoder.finish()]);
        Ok(())
    }

    /// Encodes all uploads and passes without submitting, for composition with output.
    /// Submit before preparing another frame. Errors leave resident state and the
    /// encoder unchanged; a successful encoder must be submitted, not abandoned.
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
        let (color, depth) = attachments;
        let frame = FrameUniform::new(view, Some(&self.effects))?;
        let matrix = self
            .gpu
            .caster_matrix(self.settings.matrix(frame.sun_direction)?);
        if !matrix.is_finite() || matrix.determinant() == 0.0 {
            return Err(ShadowError::InvalidSettings);
        }
        let shadow = ResourceRef::new("shadow depth");
        let color_id = crate::compositor::attachment_resource("scene color", color);
        let depth_id = crate::compositor::attachment_resource("scene depth", depth);
        let shadow_view = self.map.depth.layer_view(0)?;
        let mut shadow_pass = PassDeclaration::new(PassSlot::ShadowCasters(0));
        let shadow_view = AttachmentView::new(shadow, shadow_view.texture())?;
        shadow_pass.depth = Some(crate::compositor::attachment(shadow, &shadow_view, true));
        let mut opaque = PassDeclaration::new(PassSlot::Opaque);
        opaque.color = Some(crate::compositor::attachment(color_id, color, false));
        opaque.depth = Some(crate::compositor::attachment(depth_id, depth, true));
        let views = [
            shadow_view,
            color.with_resource(color_id)?,
            depth.with_resource(depth_id)?,
        ];
        let mut feature = ShadowFeature {
            gpu: &mut self.gpu,
            pipelines: &self.pipelines,
            depth_pipelines: &self.depth_pipelines,
            depth_frame_group: &self.depth_frame_group,
            shadow_group: &self.map.group,
            matrix,
            frame,
            depth_frame: &self.depth_frame,
            shadow_uniform: &self.map.uniform,
            settings: self.settings,
        };
        let mut declaration = FeatureDeclaration::new(PassSlot::ShadowCasters(0));
        declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
        let mut opaque_feature = crate::compositor::SceneFeature::declaration()
            .passes
            .remove(0);
        opaque_feature.reads.push(shadow);
        declaration.passes.push(opaque_feature);
        scheduler.unwrap_or(&mut self.scheduler).encode(
            context.device(),
            encoder,
            &[shadow_pass, opaque],
            &views,
            &mut [BorrowedFeature {
                declaration,
                prepared: &mut feature,
            }],
        )?;
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
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shadowed capture"),
        });
    renderer.encode(
        context,
        view,
        &target.color_attachment(),
        &target.depth_attachment(),
        &mut encoder,
    )?;
    target.encode_readback(&mut encoder)?;
    let submission = context.queue().submit([encoder.finish()]);
    Ok(target.read_encoded(context, submission)?)
}

struct ShadowFeature<'a> {
    gpu: &'a mut SceneGpu,
    pipelines: &'a ScenePipelines,
    depth_pipelines: &'a [wgpu::RenderPipeline; 4],
    depth_frame_group: &'a wgpu::BindGroup,
    shadow_group: &'a wgpu::BindGroup,
    matrix: glam::Mat4,
    frame: FrameUniform,
    depth_frame: &'a wgpu::Buffer,
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
        // Billboards retain the camera position in both passes.
        let light_frame = FrameUniform {
            view_projection: self.matrix,
            ..self.frame
        };
        context.upload_buffer(self.depth_frame, 0, &light_frame.to_bytes());
        context.upload_buffer(
            self.shadow_uniform,
            0,
            &resources::uniform_bytes(self.matrix, self.settings),
        );
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if context.slot == worldrunner_render_post::PassSlot::ShadowCasters(0) {
            self.gpu.draw_objects(
                pass,
                self.depth_frame_group,
                self.depth_pipelines,
                Some(self.matrix),
            );
        } else {
            pass.set_bind_group(2, self.shadow_group, &[]);
            self.gpu.draw(pass, self.pipelines);
        }
    }
}

#[cfg(test)]
mod tests;
