use super::output::MetadataOutput;
use crate::renderer::ScenePipelines;

fn camera_group(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("reflection prepass camera"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("reflection prepass camera"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    (layout, group)
}

pub(crate) fn pipelines(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    output: MetadataOutput,
) -> (ScenePipelines, wgpu::BindGroup) {
    pipelines_from_sources(device, uniform, &sources(output))
}

pub(crate) fn sources(output: MetadataOutput) -> (String, String) {
    let camera = include_str!("../ssao/camera.wgsl");
    let object_source = crate::shader::ENVIRONMENT_WGSL
        .replace("fn fs_relief_opaque(", "fn unused_relief_opaque(")
        .replace("fn fs_relief_masked(", "fn unused_relief_masked(")
        .replace("fn fs_opaque(", "fn unused_opaque(")
        .replace("fn fs_masked(", "fn unused_masked(");
    let object = format!("{object_source}\n{camera}\n{}", output.object_fragment());
    let terrain = format!(
        "{}\n{camera}",
        output.terrain_source(crate::shader::TERRAIN_WGSL)
    );
    (object, terrain)
}

pub(crate) fn pipelines_from_sources(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    sources: &(String, String),
) -> (ScenePipelines, wgpu::BindGroup) {
    let (layout, group) = camera_group(device, uniform);
    (
        ScenePipelines::with_screen_space(
            device,
            (wgpu::TextureFormat::Rgba16Float, 1),
            &sources.0,
            &sources.1,
            None,
            Some(&layout),
        )
        .with_fixed_terrain(),
        group,
    )
}
