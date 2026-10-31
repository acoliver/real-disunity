pub(super) fn filter_target(
    source: String,
    scalable: bool,
    gather: bool,
) -> (String, wgpu::TextureFormat) {
    if gather {
        (
            filter_source(&source, scalable),
            wgpu::TextureFormat::Rg32Float,
        )
    } else {
        (
            source,
            if scalable {
                wgpu::TextureFormat::Rg16Float
            } else {
                wgpu::TextureFormat::R32Float
            },
        )
    }
}

use worldrunner_render_plan::SceneSsaoReceiver;

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
    assert!(source.contains(function), "nearest receiver source");
    let source = source
        .replace(function, replacement)
        .replace(
            "return material_ao * screen;",
            "return min(1.01 * screen, material_ao);",
        )
        .replace(
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

pub(super) fn filter_source(source: &str, scalable: bool) -> String {
    if scalable {
        source.replace("centre.y);", "textureLoad(depths, pixel, 0).r);")
    } else {
        source
            .replace("-> @location(0) f32", "-> @location(0) vec2<f32>")
            .replace(
                "return sum / weights;",
                "return vec2<f32>(sum / weights, textureLoad(depths, pixel, 0).r);",
            )
            .replace(
                "return sum / total;",
                "return vec2<f32>(sum / total, textureLoad(depths, pixel, 0).r);",
            )
    }
}

pub(super) fn filter_depth(source: String, metadata: bool) -> String {
    if !metadata {
        return source;
    }
    source
        .replace(
            "textureLoad(depths, pixel, 0).r",
            "textureLoad(depths, pixel, 0).a",
        )
        .replace("textureLoad(depths, p, 0).r", "textureLoad(depths, p, 0).a")
}
