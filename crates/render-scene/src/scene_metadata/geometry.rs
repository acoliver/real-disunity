use crate::renderer::ScenePipelines;

pub(crate) fn pipelines(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    material: bool,
) -> (ScenePipelines, wgpu::BindGroup) {
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
    let camera = include_str!("../ssao/camera.wgsl");
    let object_source = crate::shader::ENVIRONMENT_WGSL
        .replace("fn fs_relief_opaque(", "fn unused_relief_opaque(")
        .replace("fn fs_relief_masked(", "fn unused_relief_masked(")
        .replace("fn fs_opaque(", "fn unused_opaque(")
        .replace("fn fs_masked(", "fn unused_masked(");
    let output = if material {
        "vec4<f32>(mix(vec3<f32>(0.04), albedo.rgb, metalness), roughness)"
    } else {
        "vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(normal, 0.0)).xyz), 1.0)"
    };
    let object = format!(
        "{object_source}\n{camera}\n{}",
        include_str!("geometry.wgsl").replace("METADATA_OUTPUT", output)
    );
    let terrain_output = if material {
        "vec4<f32>(mix(vec3<f32>(0.04), blended.albedo, blended.metalness), blended.roughness)"
    } else {
        "vec4<f32>(normalize((screen.view * vec4<f32>(normal, 0.0)).xyz), 1.0)"
    };
    let marker =
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);";
    assert_eq!(crate::shader::TERRAIN_WGSL.matches(marker).count(), 1);
    let terrain = format!(
        "{}\n{camera}",
        crate::shader::TERRAIN_WGSL.replace(marker, &format!("return {terrain_output};"))
    );
    (
        ScenePipelines::with_screen_space(
            device,
            (wgpu::TextureFormat::Rgba16Float, 1),
            &object,
            &terrain,
            None,
            Some(&layout),
        ),
        group,
    )
}
