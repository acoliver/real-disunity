pub(super) fn filter_target(
    source: String,
    filter: SceneSsaoFilter,
    gather: bool,
) -> (String, wgpu::TextureFormat) {
    if gather {
        (
            filter_source(&source, filter),
            wgpu::TextureFormat::Rg32Float,
        )
    } else {
        (
            source,
            if matches!(filter, SceneSsaoFilter::ScalableBilateral { .. }) {
                wgpu::TextureFormat::Rg16Float
            } else {
                wgpu::TextureFormat::R32Float
            },
        )
    }
}

use crate::shader::rewrite::ShaderRewrite;
use worldrunner_render_plan::{SceneSsaoFilter, SceneSsaoReceiver};

pub(crate) const fn gathers(receiver: SceneSsaoReceiver) -> bool {
    matches!(receiver, SceneSsaoReceiver::DepthAwareGather)
}

pub(crate) fn source(source: &str, receiver: SceneSsaoReceiver) -> String {
    if !gathers(receiver) {
        return source.to_owned();
    }
    let nearest = super::ambient_source("/* SCREEN_AMBIENT_FACTOR */");
    let function = nearest
        .split("fn screen_ao")
        .nth(1)
        .expect("nearest receiver");
    let replacement = "(world: vec3<f32>) -> f32 {
    let clip = frame.view_projection * vec4<f32>(world, 1.0);
    let uv = clip.xy / clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return receiver_gather(uv, -(ao_receiver.view * vec4<f32>(world, 1.0)).z);
}";
    let source = source
        .anchored("ssao::receiver::source", 1, function, replacement)
        .anchored(
            "ssao::receiver::source",
            1,
            "return material_ao * screen;",
            "return min(1.01 * screen, material_ao);",
        )
        .anchored(
            "ssao::receiver::source",
            1,
            "ambient_occlusion * scene_screen_ao(world_position)",
            "min(1.01 * scene_screen_ao(world_position), ambient_occlusion)",
        );
    format!(
        "{source}\nstruct AoReceiver {{ view: mat4x4<f32>, }}\n@group(3) @binding(5) var<uniform> ao_receiver: AoReceiver;\n{}",
        include_str!("receiver.wgsl")
    )
}

pub(crate) fn uniform_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 5,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("depth-aware AO receiver"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            uniform_entry(),
        ],
    })
}

pub(crate) fn group(
    device: &wgpu::Device,
    filtered: &wgpu::Texture,
    uniform: &wgpu::Buffer,
    receiver: SceneSsaoReceiver,
) -> wgpu::BindGroup {
    let view = filtered.create_view(&wgpu::TextureViewDescriptor::default());
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&view),
    }];
    let layout = if gathers(receiver) {
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: uniform.as_entire_binding(),
        });
        layout(device)
    } else {
        super::ambient_layout(device)
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("filtered ambient occlusion"),
        layout: &layout,
        entries: &entries,
    })
}

/// The final filter of `filter` with the consumed depth beside the occlusion: the box filter
/// (`blur.wgsl`) returns `sum / weights`, the separable bilateral `sum / total`.
pub(super) fn filter_source(source: &str, filter: SceneSsaoFilter) -> String {
    let result = match filter {
        SceneSsaoFilter::ScalableBilateral { .. } => {
            return source.anchored(
                "ssao::receiver::filter_source",
                1,
                "centre.y);",
                "textureLoad(depths, pixel, 0).r);",
            );
        }
        SceneSsaoFilter::DepthAwareBox => "sum / weights",
        SceneSsaoFilter::SeparableBilateral => "sum / total",
    };
    source
        .anchored(
            "ssao::receiver::filter_source",
            1,
            "-> @location(0) f32",
            "-> @location(0) vec2<f32>",
        )
        .anchored(
            "ssao::receiver::filter_source",
            1,
            &format!("return {result};"),
            &format!("return vec2<f32>({result}, textureLoad(depths, pixel, 0).r);"),
        )
}

/// Reads the depth of an RGBA metadata target: the filter's centre load, the gathered
/// filter's depth output when `gather`, and every tap.
pub(super) fn filter_depth(source: String, metadata: bool, gather: bool) -> String {
    if !metadata {
        return source;
    }
    source
        .anchored(
            "ssao::receiver::filter_depth",
            1 + usize::from(gather),
            "textureLoad(depths, pixel, 0).r",
            "textureLoad(depths, pixel, 0).a",
        )
        .anchored(
            "ssao::receiver::filter_depth",
            1,
            "textureLoad(depths, p, 0).r",
            "textureLoad(depths, p, 0).a",
        )
}
