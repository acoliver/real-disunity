pub(crate) fn build(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    material: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let module = super::scene_shader(
        device,
        "native UI overlay",
        &format!(
            "{}\n{}",
            crate::shader::ENVIRONMENT_WGSL,
            include_str!("../shader/overlay.wgsl")
        ),
    );
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("native UI overlay"),
        bind_group_layouts: &[None, Some(material)],
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
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("native UI overlay"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_ui_overlay"),
            buffers: &buffers,
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[
                    ("UI_WIDTH", f64::from(size[0])),
                    ("UI_HEIGHT", f64::from(size[1])),
                ],
                ..Default::default()
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
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
