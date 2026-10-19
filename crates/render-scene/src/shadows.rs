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
//! renderer.render(context, view, target.color_view(), target.depth_view())?;
//! let image = target.read_rgba8(context)?;
//! # Ok(())
//! # }
//! ```

mod resources;
mod settings;

pub use settings::{DirectionalShadowSettings, MAX_SHADOW_RESOLUTION};

use worldrunner_render_plan::{RenderScene, SceneEffects};

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
    /// Resolution exceeds the explicit resource ceiling or device limit.
    #[error("shadow resolution {requested} is outside 1..={limit}")]
    Resolution {
        /// Requested square texture dimension.
        requested: u32,
        /// Effective maximum dimension.
        limit: u32,
    },
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
        settings.validate_light(scene.effects())?;
        let shadow_layout = resources::layout(context.device());
        let pipelines = ScenePipelines::with_environment(
            context.device(),
            OffscreenTarget::COLOR_FORMAT,
            SHADOWED_WGSL,
            Some(&shadow_layout),
        );
        let depth_pipelines = resources::depth_pipelines(context.device(), &pipelines);
        let gpu = SceneGpu::upload(context.device(), context.queue(), scene, &pipelines)?;
        let depth_frame = upload_buffer(
            context.device(),
            context.queue(),
            "shadow frame",
            wgpu::BufferUsages::UNIFORM,
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
        let map = ShadowMap::new(context, settings, &shadow_layout);
        Ok(Self {
            gpu,
            pipelines,
            depth_pipelines,
            depth_frame,
            depth_frame_group,
            shadow_layout,
            map,
            settings,
            effects: scene.effects().clone(),
        })
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
            self.map = ShadowMap::new(context, settings, &self.shadow_layout);
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
    /// No host allocation or readback occurs. Use the same device as construction.
    pub fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> Result<(), ShadowError> {
        let frame = FrameUniform::new(view, &self.effects)?;
        let matrix = self.settings.matrix(frame.sun_direction)?;
        self.gpu.set_frame(context.queue(), frame);
        // Keep the actual camera position: billboards must face the camera in
        // BOTH passes, not rotate toward the light during shadow rendering.
        let light_frame = FrameUniform {
            view_projection: matrix,
            ..frame
        };
        context
            .queue()
            .write_buffer(&self.depth_frame, 0, &light_frame.to_bytes());
        context.queue().write_buffer(
            &self.map.uniform,
            0,
            &resources::uniform_bytes(matrix, self.settings),
        );
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("shadowed scene"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("directional shadow depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth_attachment(&self.map.view)),
                ..Default::default()
            });
            self.gpu
                .draw_objects(&mut pass, &self.depth_frame_group, &self.depth_pipelines);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadowed scene colour"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(depth_attachment(depth)),
                ..Default::default()
            });
            pass.set_bind_group(2, &self.map.group, &[]);
            self.gpu.draw(&mut pass, &self.pipelines);
        }
        context.queue().submit([encoder.finish()]);
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
    settings.validate_light(scene.effects())?;
    let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
    let mut renderer = ShadowedScene::new(context, scene, settings)?;
    renderer.render(context, view, target.color_view(), target.depth_view())?;
    Ok(target.read_rgba8(context)?)
}

fn depth_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

#[cfg(test)]
mod tests;
