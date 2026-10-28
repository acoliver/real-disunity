use std::fmt::Write;
/// A negative cutoff disables rejection while retaining its magnitude for LOD.
pub(super) fn roughness_cutoff(settings: &worldrunner_render_plan::SceneSsr) -> f32 {
    let cutoff = scalar(settings.roughness_cutoff_micro) / 1_000_000.0;
    if settings.roughness_cutoff_disabled {
        -cutoff
    } else {
        cutoff
    }
}
pub(super) fn reduction_pipeline(
    device: &wgpu::Device,
    samples: u32,
    settings: &worldrunner_render_plan::SceneSsr,
) -> wgpu::RenderPipeline {
    pipeline(
        device,
        &super::pyramid::reduction_source(samples, settings),
        1,
        false,
    )
}
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
        "{}\n{}\n{}",
        include_str!("controls.wgsl"),
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

pub(super) fn configured_trace_source(
    samples: u32,
    water: bool,
    rounding: worldrunner_render_plan::SceneDownsampleRounding,
) -> String {
    let source = trace_source(samples, water);
    match rounding {
        worldrunner_render_plan::SceneDownsampleRounding::Ceil => source,
        worldrunner_render_plan::SceneDownsampleRounding::Floor => source.replace(
            "let trace_size = ceil(camera.size.xy / camera.size.z);",
            "let trace_size = max(floor(camera.size.xy / camera.size.z), vec2<f32>(1.0));",
        ),
    }
}

pub(super) fn reduce_source(samples: u32) -> String {
    include_str!("reduce.wgsl").replace("SAMPLE_LANES", &samples.to_string())
}

pub(super) fn configured_reduce_source(
    samples: u32,
    rounding: worldrunner_render_plan::SceneDownsampleRounding,
) -> String {
    let source = reduce_source(samples);
    match rounding {
        worldrunner_render_plan::SceneDownsampleRounding::Ceil => source,
        worldrunner_render_plan::SceneDownsampleRounding::Floor => source.replace(
            "let target_width = (size.x + 1) / 2;",
            "let target_width = max(size.x / 2, 1);",
        ),
    }
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
    settings: &worldrunner_render_plan::SceneSsr,
) -> Vec<wgpu::Texture> {
    if super::pyramid::shared(settings) {
        return super::pyramid::textures(device, config, settings);
    }
    settings
        .radiance_extents(config.size)
        .into_iter()
        .map(|extent| {
            crate::ssao::passes::texture(
                device,
                [extent[0] * config.samples, extent[1]],
                wgpu::TextureFormat::Rgba16Float,
            )
        })
        .collect()
}

pub(super) fn controls(settings: &worldrunner_render_plan::SceneSsr) -> [f32; 12] {
    [
        scalar(settings.max_distance_millimeters) / 1000.0,
        scalar(settings.thickness_slope_micro) / 1_000_000.0,
        scalar(settings.origin_skip_pixels),
        scalar(settings.pass_behind_steps),
        settings.roughness_fade_start_micro.map_or(
            scalar(settings.roughness_cutoff_micro) / 1_000_000.0 * 0.75,
            |value| scalar(value) / 1_000_000.0,
        ),
        settings
            .luminance_limit_milli
            .map_or(0.0, |value| scalar(value) / 1000.0),
        f32::from(u8::from(
            settings.traversal == worldrunner_render_plan::SceneSsrTraversal::Grid,
        )),
        f32::from(u8::from(
            settings.application_lod
                == worldrunner_render_plan::SceneSsrLodMapping::RoughnessDownsample,
        )),
        settings
            .roughness_pass_behind
            .map_or(-1.0, |c| scalar(c.minimum_steps)),
        settings
            .roughness_pass_behind
            .map_or(0.0, |c| scalar(c.maximum_steps)),
        settings
            .roughness_pass_behind
            .map_or(0.0, |c| scalar(c.roughness_start_micro) / 1_000_000.0),
        settings
            .roughness_pass_behind
            .map_or(1.0, |c| scalar(c.roughness_end_micro) / 1_000_000.0),
    ]
}

pub(super) fn configured_resolve_source(
    levels: usize,
    samples: u32,
    water: bool,
    ao: bool,
    packed_depth: bool,
) -> String {
    let source = sampled_resolve_source(levels, samples, water);
    if !ao {
        return source;
    }
    let cosine = "clamp(dot(reflection_normal(normal.xyz), -reflection_incident(uv)), 0.0, 1.0)";
    let published = "textureLoad(forward_environment, p, FORWARD_SAMPLE).a";
    let source = source
        .replace("var normals: texture_2d<f32>;", "var application_ao: texture_2d<f32>;")
        .replace("textureLoad(normals, p, 0)", &format!("vec4<f32>(0.0, 0.0, 1.0, select(0.0, 1.0, {published} > 0.0))"))
        .replace(&format!("let cosine = {cosine};"), &if water { format!("let cosine = select({published}, {cosine}, is_water);") } else { format!("let cosine = {published};") })
        .replace("// APPLICATION_AO", if water {
            "if !is_water { let visibility = gathered_visibility(p, cosine, surface.a, sample); factor *= visibility; }"
        } else {
            "let visibility = gathered_visibility(p, cosine, surface.a, sample); factor *= visibility;"
        });
    format!(
        "{source}\n{}\n{}",
        include_str!("application_ao.wgsl"),
        include_str!("ao_math.wgsl")
    )
    .replace(
        "APPLICATION_TAP_DEPTH",
        if packed_depth {
            "tap.g * reflection_far_depth()"
        } else {
            "textureLoad(opaque_depth, coordinates[i], 0).r"
        },
    )
    .replace(
        "FORWARD_SAMPLE",
        if samples == 1 { "0" } else { "i32(sample)" },
    )
}

pub(super) fn resolve_pipelines(
    device: &wgpu::Device,
    samples: u32,
    levels: usize,
    ao: bool,
    packed_depth: bool,
    shared_extents: Option<&[[u32; 2]]>,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let source = |water| {
        let source = configured_resolve_source(levels, samples, water, ao, packed_depth);
        shared_extents.map_or_else(
            || source.clone(),
            |extents| super::pyramid::resolve_source(source.clone(), extents, samples),
        )
    };
    (
        pipeline(device, &source(false), samples, true),
        pipeline(device, &source(true), samples, true),
    )
}
