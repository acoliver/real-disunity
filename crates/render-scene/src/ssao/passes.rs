use crate::frame::metadata::FrameProduct;
use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FeaturePass, FrameError, PassDeclaration, PassSlot,
    PreparedFeature, ResourceRef,
};

pub(crate) fn texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screen space target"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(super) fn pipeline(device: &wgpu::Device, shader: &str) -> wgpu::RenderPipeline {
    let source = format!("{}\n{shader}", include_str!("fullscreen.wgsl"));
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("screen space"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("screen space"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_fullscreen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_screen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    uniform: Option<&wgpu::Buffer>,
    textures: &[&wgpu::Texture],
) -> wgpu::BindGroup {
    let views: Vec<_> = textures
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let mut entries = Vec::new();
    if let Some(uniform) = uniform {
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        });
    }
    for (index, view) in views.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: u32::try_from(index + 1).expect("bounded bindings"),
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("screen inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    })
}

#[derive(Clone, Debug)]
struct ScreenPass {
    slot: PassSlot,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    target: wgpu::Texture,
    reads: Vec<ResourceRef>,
}
#[derive(Clone, Debug)]
pub(crate) struct ScreenPasses {
    stages: Vec<ScreenPass>,
    pub filtered: wgpu::Texture,
}
fn level(index: u32) -> ResourceRef {
    FrameProduct::linear_depth(index).resource()
}
impl ScreenPasses {
    pub(crate) fn textures(&self) -> Vec<&wgpu::Texture> {
        self.stages.iter().map(|stage| &stage.target).collect()
    }

    pub(crate) fn schedule_metadata(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let stage = &self.stages[0];
        let resource = Self::resource(stage.slot);
        let view = AttachmentView::new(resource, &stage.target)?;
        let mut pass = PassDeclaration::new(stage.slot);
        pass.color = Some(crate::compositor::attachment(resource, &view, false));
        passes.push(pass);
        views.push(view);
        let mut feature = FeaturePass::new(stage.slot);
        feature.color_format = Some(wgpu::TextureFormat::R32Float);
        feature.reads.clone_from(&stage.reads);
        Ok(FeatureDeclaration {
            passes: vec![feature],
            effects: vec![],
        })
    }

    pub(crate) fn depth_levels(&self) -> Vec<&wgpu::Texture> {
        self.stages
            .iter()
            .filter(|stage| matches!(stage.slot, PassSlot::DepthPyramid(_)))
            .map(|stage| &stage.target)
            .collect()
    }

    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        uniform: &wgpu::Buffer,
        depth: &wgpu::Texture,
        normals: &wgpu::Texture,
    ) -> Self {
        let mut stages: Vec<ScreenPass> = Vec::new();
        let mut extent = size;
        let mut index = 0;
        loop {
            let target = texture(device, extent, wgpu::TextureFormat::R32Float);
            let (shader, input, parameter, reads) = if index == 0 {
                (
                    include_str!("linear.wgsl"),
                    depth,
                    Some(uniform),
                    vec![FrameProduct::prepass_depth().resource()],
                )
            } else {
                (
                    include_str!("reduce.wgsl"),
                    &stages.last().expect("base depth exists").target,
                    None,
                    vec![level(index - 1)],
                )
            };
            let pipeline = pipeline(device, shader);
            let group = group(device, &pipeline, parameter, &[input]);
            stages.push(ScreenPass {
                slot: PassSlot::DepthPyramid(index),
                pipeline,
                group,
                target,
                reads,
            });
            if extent == [1, 1] {
                break;
            }
            extent = [extent[0].div_ceil(2), extent[1].div_ceil(2)];
            index += 1;
        }
        let raw = texture(device, size, wgpu::TextureFormat::R32Float);
        let ao = pipeline(device, include_str!("occlusion.wgsl"));
        let ao_group = group(device, &ao, Some(uniform), &[&stages[0].target, normals]);
        let filtered = texture(device, size, wgpu::TextureFormat::R32Float);
        let blur = pipeline(device, include_str!("blur.wgsl"));
        let blur_group = group(device, &blur, Some(uniform), &[&stages[0].target, &raw]);
        stages.push(ScreenPass {
            slot: PassSlot::Ssao,
            pipeline: ao,
            group: ao_group,
            target: raw,
            reads: vec![level(0), FrameProduct::normals().resource()],
        });
        stages.push(ScreenPass {
            slot: PassSlot::SsaoBlur,
            pipeline: blur,
            group: blur_group,
            target: filtered.clone(),
            reads: vec![level(0), ResourceRef::new("raw ambient occlusion")],
        });
        Self { stages, filtered }
    }
    fn resource(slot: PassSlot) -> ResourceRef {
        match slot {
            PassSlot::DepthPyramid(index) => level(index),
            PassSlot::Ssao => ResourceRef::new("raw ambient occlusion"),
            PassSlot::SsaoBlur => ResourceRef::new("ambient occlusion"),
            _ => unreachable!("screen slot"),
        }
    }
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let mut declaration = FeatureDeclaration {
            passes: Vec::new(),
            effects: vec!["SSAO"],
        };
        for stage in &self.stages {
            let resource = Self::resource(stage.slot);
            let view = AttachmentView::new(resource, &stage.target)?;
            let mut pass = PassDeclaration::new(stage.slot);
            pass.color = Some(crate::compositor::attachment(resource, &view, false));
            passes.push(pass);
            views.push(view);
            let mut feature = FeaturePass::new(stage.slot);
            feature.color_format = Some(wgpu::TextureFormat::R32Float);
            feature.reads.clone_from(&stage.reads);
            declaration.passes.push(feature);
        }
        Ok(declaration)
    }
}

pub(crate) struct ScreenFeature<'a> {
    pub ssao: &'a super::Ssao,
    pub frame: crate::frame::FrameUniform,
}
impl PreparedFeature for ScreenFeature<'_> {
    fn prepare(
        &mut self,
        _: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.ssao.upload(context, self.frame);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let stage = self
            .ssao
            .passes
            .stages
            .iter()
            .find(|stage| stage.slot == context.slot)
            .expect("declared screen slot");
        pass.set_pipeline(&stage.pipeline);
        pass.set_bind_group(0, &stage.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub(crate) fn prepass(
    ssao: &super::Ssao,
    passes: &mut Vec<PassDeclaration>,
    views: &mut Vec<AttachmentView>,
) -> Result<(), FrameError> {
    let normals = FrameProduct::normals().attachment(&ssao.normals)?;
    let depth = FrameProduct::prepass_depth().attachment(&ssao.depth)?;
    let mut pass = PassDeclaration::new(PassSlot::DepthNormalPrepass);
    pass.color = Some(crate::compositor::attachment(
        normals.resource(),
        &normals,
        false,
    ));
    // Uncovered normals have alpha zero, including sky pixels.
    pass.color.as_mut().expect("normals").load =
        worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
    pass.depth = Some(crate::compositor::attachment(
        depth.resource(),
        &depth,
        true,
    ));
    passes.push(pass);
    views.extend([normals, depth]);
    Ok(())
}

pub(crate) fn geometry_declaration(declaration: &mut FeatureDeclaration) {
    let mut prepass = FeaturePass::new(PassSlot::DepthNormalPrepass);
    prepass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
    prepass.depth_format = Some(wgpu::TextureFormat::Depth32Float);
    declaration.passes.push(prepass);
    declaration
        .passes
        .iter_mut()
        .find(|pass| pass.slot == PassSlot::Opaque)
        .expect("opaque feature")
        .reads
        .push(ResourceRef::new("ambient occlusion"));
}

impl ScreenFeature<'_> {
    pub(crate) fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let ssao = self.ssao;
        if passes
            .iter()
            .find(|pass| pass.slot == PassSlot::Opaque)
            .and_then(|pass| pass.color.as_ref())
            .is_none_or(|color| color.size != ssao.size)
        {
            return Err(FrameError::SsaoSettings(
                "prepass and opaque render extents must match",
            ));
        }
        let mut screen_passes = Vec::new();
        crate::ssao::passes::prepass(ssao, &mut screen_passes, views)?;
        let declaration = ssao.passes.schedule(&mut screen_passes, views)?;
        let insertion = passes
            .iter()
            .position(|pass| pass.slot >= PassSlot::DepthNormalPrepass)
            .unwrap_or(passes.len());
        passes.splice(insertion..insertion, screen_passes);

        Ok(declaration)
    }
}

pub(super) fn source_textures(
    device: &wgpu::Device,
    size: [u32; 2],
) -> Result<(wgpu::Texture, wgpu::Texture), FrameError> {
    let normals = texture(device, size, wgpu::TextureFormat::Rgba16Float);
    let depth = texture(device, size, wgpu::TextureFormat::Depth32Float);
    crate::frame::metadata::validate_inputs(
        &depth,
        &normals,
        None,
        None,
        crate::frame::metadata::MetadataSampling::PixelCenter,
    )
    .map_err(|_| FrameError::SsaoSettings("source metadata contract"))?;
    Ok((normals, depth))
}
