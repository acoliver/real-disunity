use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FeaturePass, PassDeclaration, PassSlot, ResourceRef,
};

#[derive(Debug)]
pub(super) struct Target {
    pub color: AttachmentView,
    pub depth: AttachmentView,
    pub group: wgpu::BindGroup,
}
impl Target {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        samples: u32,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("celestial visibility"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let color = AttachmentView::new(ResourceRef::new("celestial radiance"), &color)
            .expect("celestial color attachment");
        let depth = AttachmentView::new(ResourceRef::new("celestial depth"), &depth)
            .expect("celestial depth attachment");
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("celestial composition"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(color.view()),
            }],
        });
        Self {
            color,
            depth,
            group,
        }
    }
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
        declaration: &mut FeatureDeclaration,
    ) {
        let mut pass = PassDeclaration::new(PassSlot::Celestial);
        pass.color = Some(crate::compositor::attachment(
            self.color.resource(),
            &self.color,
            false,
        ));
        pass.color.as_mut().expect("color").load =
            worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
        pass.depth = Some(crate::compositor::attachment(
            self.depth.resource(),
            &self.depth,
            true,
        ));
        passes.insert(0, pass);
        views.extend([self.color.clone(), self.depth.clone()]);
        let mut feature = FeaturePass::new(PassSlot::Celestial);
        feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        feature.depth_format = Some(wgpu::TextureFormat::Depth32Float);
        feature.samples = self.color.texture().sample_count();
        declaration.passes.push(feature);
        declaration
            .passes
            .iter_mut()
            .find(|p| p.slot == PassSlot::Opaque)
            .expect("opaque consumer")
            .reads
            .push(self.color.resource());
    }
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    scene: &crate::renderer::ScenePipelines,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("celestial radiance"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: scene.samples > 1,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("celestial composition"),
        bind_group_layouts: &[Some(&group)],
        immediate_size: 0,
    });
    let mut source = include_str!("mesh_composite.wgsl").to_owned();
    if scene.samples > 1 {
        source = source.replace("texture_2d<f32>", "texture_multisampled_2d<f32>");
    }
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("celestial composition"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("celestial composition"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(super::depth_state()),
        multisample: wgpu::MultisampleState {
            count: scene.samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: scene.color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, group)
}
