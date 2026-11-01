//! The water surface as a producer of the temporal reactive mask.
//!
//! The water pass already writes four `Rgba16Float` outputs, which is the 32 byte colour
//! attachment limit of a default device, so the mask cannot be a fifth output of that pass. A
//! frame that carries a mask (`FrameTargets::temporal_mask_texture`, one sample) and a water
//! surface with the producer switched on schedules a second pass, `TransparentWaterMask`, right
//! after the water pass. Its only colour target is the mask, blended with `Max` on both channels
//! so that a later producer can raise a value and never lower it. The pass redraws the water
//! bodies with the water vertex stage and a constant fragment, against the depth the water pass
//! left behind: `LessEqual` with depth writes off, so a texel the water lost to nearer geometry
//! writes nothing.
//!
//! Clear ownership: the host clears the mask once per frame before any producer
//! (`FrameTargets::encode_mask_clear`). The pass declares the mask with `Load` and as a view with
//! retained contents, which is how the frame validator learns that the host clear produced it. A
//! pass with `ClearColor` would be correct only while it is the first writer of every frame, and
//! the cancelled-frame guarantee (a mask written in a frame that never submits is cleared again
//! by the next frame) lives in the host clear alone. Pipelines, passes and attachment lists of a
//! frame without a mask are those of a renderer without temporal masks.
use super::{Water, WaterFeature};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeaturePass, FrameError, PassDeclaration, PassSlot,
    ResourceRef, TEMPORAL_MASK_FORMAT,
};

/// Reactive strength of a water fragment. A policy constant, not derived from the animation: a
/// full 1.0 would discard the history on water and show the aliasing the history removes.
pub(crate) const REACTIVE: f32 = 0.9;
const ENTRY: &str = "fs_water_reactive";

/// Producer switch (default off) and the pipelines of the mask pass, keyed like the water ones.
#[derive(Clone, Debug, Default)]
pub(super) struct Reactive {
    enabled: bool,
    pipelines: Rc<RefCell<BTreeMap<[u32; 5], wgpu::RenderPipeline>>>,
}

/// What a frame needs to draw the mask pass: the mask and one pipeline per body.
#[derive(Clone, Debug)]
pub(super) struct MaskDraw {
    texture: wgpu::Texture,
    pipelines: Vec<wgpu::RenderPipeline>,
}

/// The mask output of the pass: red is the reactive value, green stays zero for water.
pub(crate) fn target() -> wgpu::ColorTargetState {
    let max = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    };
    wgpu::ColorTargetState {
        format: TEMPORAL_MASK_FORMAT,
        blend: Some(wgpu::BlendState {
            color: max,
            alpha: max,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

/// The depth test of the pass: the water pass wrote `Less`, so the visible surface is equal.
pub(crate) fn depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

fn source(terms: u32) -> String {
    let water = super::water_source(false);
    let vertex = super::vertex_seeds::specialize(&water, terms);
    format!(
        "{vertex}\n@fragment fn {ENTRY}() -> @location(0) vec4<f32> {{ return vec4<f32>({REACTIVE:?}, 0.0, 0.0, 0.0); }}\n"
    )
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    options: [u32; 5],
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water reactive mask"),
        source: wgpu::ShaderSource::Wgsl(source(options[4]).into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("water reactive mask"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let buffers = super::geometry::layouts(options[4] & super::endpoint::SPECIALIZATION != 0);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water reactive mask"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_water"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &super::terms::vertex_constants(options),
                ..Default::default()
            },
            buffers: &buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(target())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl crate::compositor::FrameRenderer {
    /// Switches the water surface's contribution to the temporal reactive mask (default off).
    /// It takes effect only on a frame that carries a mask, at one sample, with water installed.
    pub fn set_water_reactive_mask(&mut self, enabled: bool) {
        if let Some(water) = &mut self.water {
            water.reactive.enabled = enabled;
        }
    }
}

impl Water {
    /// The mask pass of the next frame: present only with the producer on, one sample and a mask.
    pub(super) fn mask_draw(
        &self,
        device: &wgpu::Device,
        mask: Option<&wgpu::Texture>,
        options: [u32; 5],
    ) -> Option<MaskDraw> {
        let texture = mask.filter(|_| self.reactive.enabled && self.samples == 1)?;
        let pipelines = self
            .settings
            .iter()
            .map(|body| {
                let mut selected = options;
                selected[4] |= super::vertex_seeds::key(body);
                self.reactive
                    .pipelines
                    .borrow_mut()
                    .entry(selected)
                    .or_insert_with(|| pipeline(device, &self.layout, selected))
                    .clone()
            })
            .collect();
        Some(MaskDraw {
            texture: texture.clone(),
            pipelines,
        })
    }
}

impl WaterFeature {
    /// Appends the mask pass after the water pass; the caller has just pushed that pass last.
    pub(super) fn schedule_mask(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<Option<FeaturePass>, FrameError> {
        let Some(mask) = &self.mask else {
            return Ok(None);
        };
        let view = AttachmentView::new(ResourceRef::new("temporal reactive mask"), &mask.texture)?
            .with_retained_contents();
        let depth = passes
            .last()
            .and_then(|water| water.depth.clone())
            .ok_or(FrameError::AttachmentContract(PassSlot::TransparentWater))?;
        let mut pass = PassDeclaration::new(PassSlot::TransparentWaterMask);
        let mut color = crate::compositor::attachment(view.resource(), &view, false);
        color.load = AttachmentLoad::Load;
        pass.color = Some(color);
        pass.depth = Some(depth);
        passes.push(pass);
        views.push(view);
        let mut feature = FeaturePass::new(PassSlot::TransparentWaterMask);
        feature.color_format = Some(TEMPORAL_MASK_FORMAT);
        feature.depth_format = Some(wgpu::TextureFormat::Depth32Float);
        Ok(Some(feature))
    }

    /// Redraws every body with the constant fragment.
    pub(super) fn draw_mask(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(mask) = &self.mask else {
            return;
        };
        for ((body, group), pipeline) in self
            .water
            .bodies
            .iter()
            .zip(&self.groups)
            .zip(&mask.pipelines)
        {
            if body.vertices != 0 {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.set_vertex_buffer(0, body.buffer.slice(..));
                if let Some(seed) = &body.seed {
                    pass.set_vertex_buffer(1, seed.slice(..));
                }
                pass.draw(0..body.vertices, 0..1);
            }
        }
    }
}
