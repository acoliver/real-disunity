use worldrunner_render_plan::{SceneDownsampleRounding, SceneSsr, SceneSsrPyramid};
use worldrunner_render_post::{FrameConfig, FrameError};

pub(super) fn resolve_pipelines(
    device: &wgpu::Device,
    config: FrameConfig,
    settings: &SceneSsr,
    packed_depth: bool,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let extents = settings.radiance_extents(config.size);
    super::gpu::resolve_pipelines(
        device,
        config.samples,
        extents.len(),
        settings.depth_relative_ao,
        packed_depth,
        shared(settings).then_some(extents.as_slice()),
    )
}

pub(super) fn shared(settings: &SceneSsr) -> bool {
    settings.radiance_pyramid == SceneSsrPyramid::Full
}

pub(crate) fn lane_grid(samples: u32) -> [u32; 2] {
    if samples == 1 {
        [1, 1]
    } else {
        [2, samples / 2]
    }
}

// The existing trace attachment writes all lanes in one pass. Packing across
// both axes retains that schedule without requiring layered rendering features.
// Ceil levels need padded native mips; a single Floor lane needs no padding.
pub(crate) fn storage_extent(config: FrameConfig, settings: &SceneSsr) -> Option<[u32; 2]> {
    let extents = settings.radiance_extents(config.size);
    let size = extents[0];
    if config.samples == 1 && settings.radiance_rounding == SceneDownsampleRounding::Floor {
        return Some(size);
    }
    let tail = 1u32.checked_shl(u32::try_from(extents.len() - 1).ok()?)?;
    let grid = lane_grid(config.samples);
    let allocation_axis = |axis: usize| {
        size[axis]
            .checked_next_power_of_two()?
            .max(if grid[axis] > 1 { tail } else { 1 })
            .checked_mul(grid[axis])
    };
    Some([allocation_axis(0)?, allocation_axis(1)?])
}

pub(super) fn validate(
    config: FrameConfig,
    settings: &SceneSsr,
    limits: &wgpu::Limits,
) -> Result<(), FrameError> {
    let levels = settings.radiance_extents(config.size).len();
    let bindings = if shared(settings) { 11 } else { levels + 10 };
    if bindings > usize::try_from(limits.max_sampled_textures_per_shader_stage).unwrap() {
        return Err(FrameError::SsrSettings(
            "reflection resolve exceeds sampled texture limit",
        ));
    }
    if shared(settings)
        && storage_extent(config, settings).is_none_or(|extent| {
            extent
                .into_iter()
                .any(|side| side > limits.max_texture_dimension_2d)
        })
    {
        return Err(FrameError::SsrSettings(
            "reflection mip storage exceeds device texture limit",
        ));
    }
    Ok(())
}

pub(super) fn textures(
    device: &wgpu::Device,
    config: FrameConfig,
    settings: &SceneSsr,
) -> Vec<wgpu::Texture> {
    let size = storage_extent(config, settings).expect("validated reflection mip storage");
    let count = settings.radiance_extents(config.size).len();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection colour mip chain"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: u32::try_from(count).expect("bounded reflection levels"),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    vec![texture; count]
}

pub(super) fn view(texture: &wgpu::Texture, mip: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: mip,
        mip_level_count: Some(1),
        ..Default::default()
    })
}

pub(super) fn trace_source(source: String, samples: u32, settings: &SceneSsr) -> String {
    if !shared(settings) {
        return source;
    }
    let columns = lane_grid(samples)[0];
    source.replace(
        "let sample = u32(pixel.x / trace_size.x);",
        &format!("let lane = vec2<u32>(pixel.xy / trace_size);\n    let sample = lane.x + lane.y * {columns}u;"),
    ).replace(
        "vec2<f32>(pixel.x - f32(sample) * trace_size.x, pixel.y) / trace_size",
        "(pixel.xy - vec2<f32>(lane) * trace_size) / trace_size",
    )
}

pub(super) fn reduction_source(samples: u32, settings: &SceneSsr) -> String {
    let source = super::gpu::configured_reduce_source(samples, settings.radiance_rounding);
    if !shared(settings) {
        return source;
    }
    let columns = lane_grid(samples)[0];
    format!("@group(0) @binding(0) var<uniform> logical_size: vec4<u32>;\n{source}")
        .replace(&format!("vec2<i32>(textureDimensions(source)) / vec2<i32>({samples}, 1)"), "vec2<i32>(logical_size.xy)")
        .replace("let sample = i32(pixel.x) / target_width;", &format!("let target_size = vec2<i32>(target_width, TARGET_HEIGHT);\n    let lane = vec2<i32>(pixel.xy) / target_size;\n    let sample = lane.x + lane.y * {columns};"))
        .replace("TARGET_HEIGHT", if settings.radiance_rounding == SceneDownsampleRounding::Ceil { "(size.y + 1) / 2" } else { "max(size.y / 2, 1)" })
        .replace("vec2<i32>(i32(pixel.x) - sample * target_width, i32(pixel.y))", "(vec2<i32>(pixel.xy) - lane * target_size)")
        .replace("vec2<i32>(sample * size.x, 0)", "lane * size")
}

pub(super) fn uniforms(
    device: &wgpu::Device,
    config: FrameConfig,
    settings: &SceneSsr,
) -> Vec<wgpu::Buffer> {
    use wgpu::util::DeviceExt;
    if !shared(settings) {
        return Vec::new();
    }
    let sizes = settings.radiance_extents(config.size);
    sizes[..sizes.len() - 1]
        .iter()
        .map(|size| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("reflection logical reduction extent"),
                contents: &[size[0], size[1], 0, 0]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
                usage: wgpu::BufferUsages::UNIFORM,
            })
        })
        .collect()
}

pub(super) fn resolve_source(mut source: String, extents: &[[u32; 2]], samples: u32) -> String {
    let levels = extents.len();
    for index in 1..levels {
        source = source.replace(
            &format!(
                "@group(0) @binding({}) var reflection_{index}: texture_2d<f32>;",
                index + 5
            ),
            "",
        );
    }
    for binding in levels + 5..levels + 11 {
        source = source.replace(
            &format!("@binding({binding})"),
            &format!("@binding({})", binding - levels + 1),
        );
    }
    source = source.replace(
        "textureDimensions(reflection_0)",
        &format!("vec2<u32>({}, {})", extents[0][0] * samples, extents[0][1]),
    );
    let start = source
        .find("fn reflected(")
        .expect("reflection sampling helper");
    let end_marker = "default: { return vec4<f32>(0.0); } } }\n";
    let end = start
        + source[start..]
            .find(end_marker)
            .expect("reflection sampling end")
        + end_marker.len();
    let sizes = extents
        .iter()
        .map(|size| format!("vec2<i32>({}, {})", size[0], size[1]))
        .collect::<Vec<_>>()
        .join(",");
    let helper = include_str!("pyramid.wgsl")
        .replace("LEVEL_COUNT", &levels.to_string())
        .replace("LOGICAL_SIZES", &sizes)
        .replace("LANE_COLUMNS", &lane_grid(samples)[0].to_string());
    source.replace_range(start..end, &helper);
    source
}
