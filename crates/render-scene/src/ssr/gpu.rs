use std::fmt::Write;
pub(super) fn scalar(value: u32) -> f32 {
    f32::from(u16::try_from(value >> 16).expect("upper half")) * 65536.0
        + f32::from(u16::try_from(value & 65535).expect("lower half"))
}
pub(super) fn pipeline(
    device: &wgpu::Device,
    shader: &str,
    samples: u32,
    additive: bool,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SSR"),
        source: wgpu::ShaderSource::Wgsl(
            format!("{}\n{shader}", include_str!("../ssao/fullscreen.wgsl")).into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SSR"),
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
                format: wgpu::TextureFormat::Rgba16Float,
                blend: additive.then_some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: if additive {
                    wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN | wgpu::ColorWrites::BLUE
                } else {
                    wgpu::ColorWrites::ALL
                },
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}
pub(super) fn group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    uniform: Option<&wgpu::Buffer>,
    views: &[wgpu::TextureView],
) -> wgpu::BindGroup {
    let mut entries = Vec::new();
    if let Some(uniform) = uniform {
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        });
    }
    entries.extend(
        views
            .iter()
            .enumerate()
            .map(|(index, view)| wgpu::BindGroupEntry {
                binding: u32::try_from(index + 1).expect("bounded SSR inputs"),
                resource: wgpu::BindingResource::TextureView(view),
            }),
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SSR inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    })
}
pub(super) fn resolve_source(levels: usize) -> String {
    let mut source = format!(
        "{}\n{}\n{}\n{}",
        worldrunner_render_lighting::WGSL,
        include_str!("replacement.wgsl"),
        include_str!("resolve.wgsl"),
        concat!(
            include_str!("projection.wgsl"),
            "\n",
            include_str!("normal.wgsl")
        )
    );
    for index in 0..levels {
        write!(
            source,
            "\n@group(0) @binding({}) var reflection_{index}: texture_2d<f32>;",
            index + 5
        )
        .expect("writing shader source");
    }
    let first = levels + 5;
    write!(source, "\n@group(0) @binding({first}) var forward_environment: FORWARD_TEXTURE;\n@group(0) @binding({}) var forward_factor: FORWARD_TEXTURE;", first + 1).expect("shader bindings");
    source.push_str(
        "\nfn reflected(level: u32, uv: vec2<f32>, sample: u32) -> vec4<f32> { switch level {",
    );
    for index in 0..levels {
        write!(source, "case {index}u: {{
            let size = vec2<i32>(textureDimensions(reflection_{index})) / vec2<i32>(SAMPLE_LANES, 1);
            let coordinates = reflection_coordinates(uv, size, exp2({index}.0));
            let offset = vec2<i32>(i32(sample) * size.x, 0);
            let a = vec2<i32>(coordinates[0]) + offset;
            let b = vec2<i32>(coordinates[1]) + offset;
            let weight = coordinates[2];
            return mix(
                mix(textureLoad(reflection_{index}, a, 0), textureLoad(reflection_{index}, vec2<i32>(b.x, a.y), 0), weight.x),
                mix(textureLoad(reflection_{index}, vec2<i32>(a.x, b.y), 0), textureLoad(reflection_{index}, b, 0), weight.x), weight.y);
        }}").expect("writing shader source");
    }
    source.push_str("default: { return vec4<f32>(0.0); } } }\n");
    source.replace("REFLECTION_LEVELS", &(levels - 1).to_string())
}

pub(super) fn trace_source(samples: u32, water: bool) -> String {
    let source = format!(
        "{}\n{}",
        include_str!("trace.wgsl"),
        concat!(
            include_str!("projection.wgsl"),
            "\n",
            include_str!("normal.wgsl")
        )
    )
    .replace("SAMPLE_LANES", &samples.to_string());
    if water {
        super::water::trace_source(&source, samples)
    } else {
        source
    }
}

pub(super) fn reduce_source(samples: u32) -> String {
    include_str!("reduce.wgsl").replace("SAMPLE_LANES", &samples.to_string())
}

pub(super) fn sampled_resolve_source(levels: usize, samples: u32, water: bool) -> String {
    let source = resolve_source(levels)
        .replace("SAMPLE_LANES", &samples.to_string())
        .replace(
            "FORWARD_TEXTURE",
            if samples == 1 {
                "texture_2d<f32>"
            } else {
                "texture_multisampled_2d<f32>"
            },
        )
        .replace(
            "FORWARD_SAMPLE",
            if samples == 1 { "0" } else { "i32(sample)" },
        );
    if water {
        super::water::resolve_source(&source, levels, samples)
    } else {
        source
    }
}

pub(super) fn radiance_textures(
    device: &wgpu::Device,
    config: worldrunner_render_post::FrameConfig,
    scale: u32,
) -> Vec<wgpu::Texture> {
    let mut extent = if scale == 500_000 {
        config.size.map(|side| side.div_ceil(2))
    } else {
        config.size
    };
    let mut reflections = Vec::new();
    loop {
        reflections.push(crate::ssao::passes::texture(
            device,
            [extent[0] * config.samples, extent[1]],
            wgpu::TextureFormat::Rgba16Float,
        ));
        if extent == [1, 1] || reflections.len() == 6 {
            break;
        }
        extent = extent.map(|side| side.div_ceil(2));
    }
    reflections
}
