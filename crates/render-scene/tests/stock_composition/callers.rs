//! The caller's own features: a sampler of the stock outputs in a later entry, and a probe that
//! joins the stock prefix.
use super::fixture::SIZE;
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, DrawContext, FeatureDeclaration,
    FeaturePass, FrameError, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
    ResourceRef,
};
use worldrunner_render_scene::wgpu;

pub const CALLER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// What the caller feature saw while its frame prepared.
#[derive(Default)]
pub struct Seen {
    pub depth: Option<wgpu::Texture>,
    pub colour: Option<wgpu::Texture>,
    pub completed: Vec<(PassEntry, bool)>,
}

/// A feature of the caller's own: a full-screen pass in a later Opaque entry with its own
/// colour target that writes the stock lit colour in RGB and the stock device depth in alpha.
pub struct Sampler {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    group: Option<wgpu::BindGroup>,
    pub reads: [ResourceRef; 2],
    seen: Rc<RefCell<Seen>>,
    pub refuse: Rc<std::cell::Cell<bool>>,
}
pub const SAMPLER_ENTRY: PassEntry = PassEntry::nth(PassSlot::Opaque, 1);
const SAMPLER_SHADER: &str = "
@group(0) @binding(0) var scene_depth: texture_depth_2d;
@group(0) @binding(1) var scene_colour: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(position.xy);
    let colour = textureLoad(scene_colour, texel, 0);
    return vec4<f32>(colour.rgb, textureLoad(scene_depth, texel, 0));
}
";
impl Sampler {
    pub fn new(device: &wgpu::Device, reads: [ResourceRef; 2], seen: Rc<RefCell<Seen>>) -> Self {
        let entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("caller sampler"),
            entries: &[
                entry(0, wgpu::TextureSampleType::Depth),
                entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("caller sampler"),
            source: wgpu::ShaderSource::Wgsl(SAMPLER_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("caller sampler"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("caller sampler"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: CALLER_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            group: None,
            reads,
            seen,
            refuse: Rc::default(),
        }
    }

    /// The caller's pass and the view of its own colour target.
    pub fn pass(target: &wgpu::Texture) -> (PassDeclaration, AttachmentView) {
        let resource = ResourceRef::new("caller colour");
        let view = AttachmentView::new(resource, target).unwrap();
        let mut pass = PassDeclaration::nth(SAMPLER_ENTRY.slot(), SAMPLER_ENTRY.occurrence());
        pass.color = Some(AttachmentDeclaration {
            resource,
            format: CALLER_FORMAT,
            size: SIZE,
            samples: 1,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
            read_only: false,
        });
        (pass, view)
    }

    pub fn declaration(&self) -> FeatureDeclaration {
        let mut pass = FeaturePass::nth(SAMPLER_ENTRY.slot(), SAMPLER_ENTRY.occurrence());
        pass.reads = self.reads.to_vec();
        pass.color_format = Some(CALLER_FORMAT);
        FeatureDeclaration {
            passes: vec![pass],
            effects: vec![],
        }
    }
}
impl PreparedFeature for Sampler {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        if self.refuse.get() {
            return Err(FrameError::FeaturePreparation {
                feature: "caller sampler",
                reason: "the caller refuses this frame".to_string(),
            });
        }
        let depth = context.resource(self.reads[0])?.sampled_view();
        let colour = context.resource(self.reads[1])?.sampled_view();
        {
            let mut seen = self.seen.borrow_mut();
            seen.depth = Some(depth.texture().clone());
            seen.colour = Some(colour.texture().clone());
            seen.completed = [
                PassEntry::new(PassSlot::DepthNormalPrepass),
                PassEntry::nth(PassSlot::DepthNormalPrepass, 1),
                PassEntry::new(PassSlot::DepthPyramid(0)),
                PassEntry::new(PassSlot::Opaque),
                SAMPLER_ENTRY,
            ]
            .map(|entry| (entry, context.completed_in_prefix(entry)))
            .to_vec();
        }
        self.group = Some(
            context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("caller sampler"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&depth),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&colour),
                        },
                    ],
                }),
        );
        Ok(())
    }
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(
            context.entry, SAMPLER_ENTRY,
            "the caller draws in its own entry"
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, self.group.as_ref(), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A caller feature whose pass joins the stock prefix. It draws nothing, and records whether
/// the prefix had completed its entry each time it prepared.
pub struct Probe {
    pub reads: ResourceRef,
    pub completed: Rc<RefCell<Vec<bool>>>,
}
pub const PROBE_ENTRY: PassEntry = PassEntry::nth(PassSlot::DepthPyramid(0), 1);
impl Probe {
    pub fn pass(target: &wgpu::Texture) -> (PassDeclaration, AttachmentView) {
        let resource = ResourceRef::new("caller prefix colour");
        let view = AttachmentView::new(resource, target).unwrap();
        let mut pass = PassDeclaration::nth(PROBE_ENTRY.slot(), PROBE_ENTRY.occurrence());
        pass.color = Some(AttachmentDeclaration {
            resource,
            format: target.format(),
            size: SIZE,
            samples: 1,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
            read_only: false,
        });
        (pass, view)
    }
    pub fn declaration(&self, target: &wgpu::Texture) -> FeatureDeclaration {
        let mut pass = FeaturePass::nth(PROBE_ENTRY.slot(), PROBE_ENTRY.occurrence());
        pass.reads = vec![self.reads];
        pass.color_format = Some(target.format());
        FeatureDeclaration {
            passes: vec![pass],
            effects: vec![],
        }
    }
}
impl PreparedFeature for Probe {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        context.resource(self.reads)?;
        self.completed
            .borrow_mut()
            .push(context.completed_in_prefix(PROBE_ENTRY));
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}
