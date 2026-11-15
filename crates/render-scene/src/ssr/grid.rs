use super::{FrameError, gpu};
use crate::shader::rewrite::ShaderRewrite;
use std::fmt::Write;
use worldrunner_render_plan::{SceneSsr, SceneSsrGridDomain, SceneSsrTraversal};

pub(super) fn domain(settings: &SceneSsr, size: [u32; 2]) -> (usize, usize) {
    let base = usize::from(
        settings.grid_domain == SceneSsrGridDomain::ReflectionOutput
            && settings.resolution_scale_micro == 500_000,
    );
    let available = chain_length(size).saturating_sub(base);
    let count = settings.grid_level_count.map_or(available.min(4), |n| {
        usize::try_from(n).expect("validated grid level count")
    });
    (base, count)
}
fn chain_length(mut size: [u32; 2]) -> usize {
    let mut count = 1;
    while size.iter().any(|&n| n > 1) {
        size = size.map(|n| n.div_ceil(2));
        count += 1;
    }
    count
}
pub(super) fn validate(
    size: [u32; 2],
    settings: &SceneSsr,
    limits: &wgpu::Limits,
) -> Result<(), FrameError> {
    if settings.traversal != SceneSsrTraversal::Grid {
        return Ok(());
    }
    if size.contains(&0) {
        return Err(FrameError::SsrSettings("grid extent must be nonzero"));
    }
    let (base, count) = domain(settings, size);
    if count == 0 || base + count > chain_length(size) {
        return Err(FrameError::SsrSettings(
            "grid levels exceed available depth chain",
        ));
    }
    // Include the three water inputs because either receiver can use this policy.
    if count + usize::from(base > 0) + 8
        > usize::try_from(limits.max_sampled_textures_per_shader_stage).unwrap()
    {
        return Err(FrameError::SsrSettings(
            "grid levels exceed device texture binding limit",
        ));
    }
    Ok(())
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    config: super::FrameConfig,
    settings: &SceneSsr,
    water: bool,
) -> wgpu::RenderPipeline {
    let dual = water && super::pyramid::banked(settings);
    gpu::pipeline_targets(
        device,
        &trace_source(config.samples, water, settings, config.size),
        1,
        false,
        if dual { 2 } else { 1 },
    )
}

pub(super) fn trace_source(
    samples: u32,
    water: bool,
    settings: &SceneSsr,
    size: [u32; 2],
) -> String {
    let banked = water && super::pyramid::banked(settings);
    let source = gpu::configured_trace_source(samples, water, settings.radiance_rounding);
    let source = match settings.hit_facing {
        worldrunner_render_plan::SceneSsrHitFacing::AnyForWaterReceivers if water && !banked => {
            super::water::hit_facing_source(&source)
        }
        worldrunner_render_plan::SceneSsrHitFacing::ShadingNormal
        | worldrunner_render_plan::SceneSsrHitFacing::AnyForWaterReceivers => source,
        worldrunner_render_plan::SceneSsrHitFacing::Any => source.anchored(
            "ssr::grid::trace_source",
            2,
            " || dot(hit_normal.xyz, direction) >= 0.0",
            "",
        ),
    };
    let source = super::pyramid::trace_source(source, samples, settings);
    if settings.traversal != SceneSsrTraversal::Grid {
        return if banked {
            super::water::banked_trace_source(&source, samples, false)
        } else {
            source
        };
    }
    let (base, count) = domain(settings, size);
    let source = configure_levels(source, count, base > 0, water);
    if base == 0 {
        return if banked {
            super::water::banked_trace_source(&source, samples, false)
        } else {
            source
        };
    }
    // Refine half-resolution bounds in the same walk before returning a child contact.
    let source = source
        .anchored("ssr::grid::trace_source", 1, "const GRID_REFINEMENT: bool = false;", "const GRID_REFINEMENT: bool = true;")
        .anchored("ssr::grid::trace_source", 1, "fn grid_child_depth(pixel: vec2<f32>) -> f32 { return grid_depth(0, pixel, vec2<f32>(1.0)); }",
            &format!("fn grid_child_depth(pixel: vec2<f32>) -> f32 {{ return textureLoad(grid_depth_{count}, vec2<i32>(pixel*2.0), 0).r; }}"))
        .anchored("ssr::grid::trace_source", 1,
            "let grid_size = camera.size.xy;",
            "let grid_size = camera.size.xy / 2.0;",
        )
        .anchored("ssr::grid::trace_source", 1,
            "let hit_pixel = vec2<i32>(intersection.pixel);",
            "let full_pixel = intersection.pixel * 2.0;
    if any(full_pixel < vec2<f32>(0.0)) || any(full_pixel >= camera.size.xy) { return vec4<f32>(0.0); }
    let hit_pixel = vec2<i32>(full_pixel);",
        )
        .anchored("ssr::grid::trace_source", 1,
            "if !matches_receiver_depth(intersection.depth, metadata_depth, hit_normal.xyz, hit_pixel)",
            &format!("if intersection.depth != textureLoad(grid_depth_{count}, hit_pixel, 0).r || !matches_receiver_depth(intersection.depth, metadata_depth, hit_normal.xyz, hit_pixel)"),
        );
    if banked {
        super::water::banked_trace_source(&source, samples, true)
    } else {
        source
    }
}
fn configure_levels(mut source: String, count: usize, ownership: bool, water: bool) -> String {
    let first = source
        .find("@group(0) @binding(4)")
        .expect("depth bindings");
    let last = source.find("fn edge_fade").expect("depth helper end");
    let bindings = count + usize::from(ownership);
    let mut declarations = String::new();
    for index in 0..bindings {
        writeln!(
            declarations,
            "@group(0) @binding(GRID_{index}) var grid_depth_{index}: texture_2d<f32>;"
        )
        .unwrap();
    }
    declarations.push_str("@group(0) @binding(8) var receiver_depth: texture_2d<f32>;\n@group(0) @binding(9) var opaque_forward_depth: texture_2d<f32>;\nfn depth_at(level: u32, uv: vec2<f32>) -> f32 { return grid_depth(i32(level), uv*camera.size.xy, camera.size.xy); }\n");
    source.replace_range(first..last, &declarations);
    // Shift existing receiver and water bindings before adding generated depth bindings. The
    // water trace declares its receiver normals, material and depth at bindings 10 to 12.
    let shifted = if water { 8..=12 } else { 8..=9 };
    for binding in shifted.clone().rev() {
        source = source.anchored(
            "ssr::grid::configure_levels",
            1,
            &format!("@binding({binding})"),
            &format!("@binding(SHIFT_{binding})"),
        );
    }
    for binding in shifted {
        source = source.anchored(
            "ssr::grid::configure_levels",
            1,
            &format!("@binding(SHIFT_{binding})"),
            &format!("@binding({})", binding + bindings - 4),
        );
    }
    for index in 0..bindings {
        source = source.anchored(
            "ssr::grid::configure_levels",
            1,
            &format!("@binding(GRID_{index})"),
            &format!("@binding({})", index + 4),
        );
    }
    configure_fetch(source, count)
}
fn configure_fetch(mut source: String, count: usize) -> String {
    let start = source.find("fn grid_depth(").expect("grid fetch");
    let end = source.find("// GRID_DEPTH_END").expect("grid fetch end");
    let mut fetch = String::from(
        "fn grid_depth(level: i32, pixel: vec2<f32>, size: vec2<f32>) -> f32 {\n let coordinate = vec2<i32>(pixel / exp2(f32(level)));\n switch level {\n",
    );
    for index in 0..count {
        let label = if index + 1 == count {
            "default".into()
        } else {
            format!("case {index}")
        };
        writeln!(fetch, "{label}: {{ return textureLoad(grid_depth_{index}, clamp(coordinate, vec2<i32>(0), vec2<i32>(textureDimensions(grid_depth_{index}))-vec2<i32>(1)), 0).r; }}").unwrap();
    }
    fetch.push_str("} }\n");
    source.replace_range(start..end, &fetch);
    source.anchored(
        "ssr::grid::configure_fetch",
        1,
        "min(3, i32(ceil(log2(max(size.x, size.y)))))",
        &format!("{}", count - 1),
    )
}
