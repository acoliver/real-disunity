pub(crate) fn build(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    material: &wgpu::BindGroupLayout,
    depth: bool,
) -> wgpu::RenderPipeline {
    let depth_layout = depth.then(|| depth_layout(device));
    let module = super::scene_shader(device, "native UI overlay", &source(depth));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("native UI overlay"),
        bind_group_layouts: &[depth_layout.as_ref(), Some(material)],
        immediate_size: 0,
    });
    let vertices = crate::geometry::vertex_attributes();
    let instances = crate::geometry::instance_attributes();
    let buffers = super::vertex_buffers(&vertices, &instances);
    let tint = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 9,
    }];
    let buffers = super::unlit::alpha_buffers(&buffers, &tint);
    pipeline(device, &module, &layout, &buffers, format, size)
}
fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("native UI overlay"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_ui_overlay"),
            buffers,
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[
                    ("UI_WIDTH", f64::from(size[0])),
                    ("UI_HEIGHT", f64::from(size[1])),
                ],
                ..Default::default()
            },
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_ui_overlay"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn depth_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("annotation scene depth"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

pub(crate) fn source(depth: bool) -> String {
    let source = include_str!("../shader/overlay.wgsl")
        .replace(
            "UI_DEPTH_BINDINGS",
            if depth {
                include_str!("../shader/overlay_depth.wgsl")
            } else {
                ""
            },
        )
        .replace(
            "UI_DEPTH_TEST",
            if depth { "test_ui_depth(input);" } else { "" },
        );
    format!("{}\n{source}", crate::shader::ENVIRONMENT_WGSL)
}
