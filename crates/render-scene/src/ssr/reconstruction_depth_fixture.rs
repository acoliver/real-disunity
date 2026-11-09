use super::*;

pub(in crate::ssr::tests) fn texture(
    context: &GpuContext,
    config: FrameConfig,
    metres: &str,
) -> wgpu::Texture {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("reconstruction raw forward depth"),
        size: wgpu::Extent3d {
            width: config.size[0],
            height: config.size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: config.samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let pipeline = pipeline(context, config.samples, metres);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    encoder.finish().submit().unwrap();
    texture
}

fn source(metres: &str) -> String {
    let projection = frame().view_projection;
    format!(
        "{}\n@fragment fn fs_depth(@builtin(position) p: vec4<f32>, @builtin(sample_index) lane: u32) -> @builtin(frag_depth) f32 {{ let z = -({metres}); return ({} * z + {}) / -z; }}",
        include_str!("../ssao/fullscreen.wgsl"),
        projection.z_axis.z,
        projection.w_axis.z,
    )
}

fn pipeline(context: &GpuContext, samples: u32, metres: &str) -> wgpu::RenderPipeline {
    let shader = source(metres);
    let module = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reconstruction depth plane"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
    context
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reconstruction depth plane"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_depth"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
}

#[test]
fn reconstruction_depth_planes_validate_without_device() {
    for metres in [
        "3.0",
        "select(3.0, 4.0, p.x > 9.0)",
        "select(3.0, 6.0, p.x > 9.0)",
        "3.0 + f32(lane)",
    ] {
        let shader = source(metres);
        let module = naga::front::wgsl::parse_str(&shader)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&shader)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
