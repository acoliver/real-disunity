use crate::scene_metadata::SceneMetadata;
use worldrunner_render_post::{
    AttachmentView, FrameConfig, FrameError, PassDeclaration, PassSlot, ResourceRef,
};

pub(crate) fn source(source: &str, terrain: bool) -> String {
    let source = source.replace("vec3<f32>(0.0); /* SCENE_ENVIRONMENT */",
        "forward_reflection(world_position, normal, f0, scene_environment_roughness(roughness_in), ao, ambient_occlusion);");
    let source = if terrain {
        source
    } else {
        source.replace(
            "forward_reflection(world_position, normal, f0, scene_environment_roughness(roughness_in), ao, ambient_occlusion)",
            "forward_reflection(world_position, scene_environment_normal(normal), f0, scene_environment_roughness(roughness_in), ao, ambient_occlusion)",
        ) + "\nfn scene_environment_normal(normal: vec3<f32>) -> vec3<f32> { if material.contact_surface.z > 0.0 { return reflection_normal(normal); } return normal; }"
    };
    let source = if terrain {
        source.replace(
            "@fragment\nfn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32>",
            "fn shaded_terrain(input: TerrainVertexOutput) -> vec4<f32>",
        ) + "\n@fragment fn fs_terrain(input: TerrainVertexOutput) -> ReflectionOutput { return reflection_output(shaded_terrain(input)); }"
    } else {
        source.replace("@fragment\nfn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32>",
            "fn shaded_opaque(input: VertexOutput, front_facing: bool) -> vec4<f32>")
            .replace("@fragment\nfn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32>",
                "fn shaded_masked(input: VertexOutput, front_facing: bool) -> vec4<f32>")
            + "\n@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> ReflectionOutput { return reflection_output(shaded_opaque(input, front)); }\n@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> ReflectionOutput { return reflection_output(shaded_masked(input, front)); }"
    };
    format!(
        "{source}\n{}\n{}",
        include_str!("normal.wgsl"),
        include_str!("forward.wgsl")
    )
}
pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = Vec::new();
    for binding in 0..3 {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float {
                    filterable: binding != 0,
                },
                view_dimension: if binding == 1 {
                    wgpu::TextureViewDimension::Cube
                } else {
                    wgpu::TextureViewDimension::D2
                },
                multisampled: false,
            },
            count: None,
        });
    }
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 3,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("reflection environment"),
        entries: &entries,
    })
}
pub(crate) fn group(
    device: &wgpu::Device,
    screen: &SceneMetadata,
    views: &[wgpu::TextureView; 2],
) -> wgpu::BindGroup {
    let ao = screen
        .passes
        .filtered
        .create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let mut entries: Vec<_> = [&ao, &views[0], &views[1]]
        .into_iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: u32::try_from(i).expect("three textures"),
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 3,
        resource: wgpu::BindingResource::Sampler(&sampler),
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("reflection environment"),
        layout: &layout(device),
        entries: &entries,
    })
}
pub(super) fn install(
    device: &wgpu::Device,
    config: FrameConfig,
    screen: &mut SceneMetadata,
    application_ao: bool,
) -> [wgpu::TextureView; 2] {
    let maps = crate::water::receivers::empty_environment(device);
    let group = group(device, screen, &maps);
    let pipelines = crate::renderer::ScenePipelines::with_screen_space(
        device,
        (config.format, config.samples),
        &application_source(
            &crate::ssao::ambient_source(crate::shader::ENVIRONMENT_WGSL),
            false,
            application_ao,
        ),
        &application_source(
            &crate::ssao::ambient_source(crate::shader::TERRAIN_WGSL),
            true,
            application_ao,
        ),
        None,
        Some(&layout(device)),
    );
    screen.geometry.forward = Some((pipelines, group));
    maps
}
pub(super) fn products(device: &wgpu::Device, config: FrameConfig) -> [wgpu::Texture; 2] {
    std::array::from_fn(|_| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("forward reflection metadata"),
            size: wgpu::Extent3d {
                width: config.size[0],
                height: config.size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: config.samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    })
}
pub(super) fn resources() -> [ResourceRef; 2] {
    [
        crate::frame::metadata::FrameProduct::forward_specular(1).resource(),
        crate::frame::metadata::FrameProduct::reflection_factor(1).resource(),
    ]
}
pub(super) fn schedule(
    products: &[wgpu::Texture; 2],
    passes: &mut [PassDeclaration],
    views: &mut Vec<AttachmentView>,
) -> Result<(), FrameError> {
    if let Some(opaque) = passes.iter_mut().find(|pass| pass.slot == PassSlot::Opaque) {
        for (resource, texture) in resources().into_iter().zip(products) {
            let product = if resource.name == "forward environment specular" {
                crate::frame::metadata::FrameProduct::forward_specular(texture.sample_count())
            } else {
                crate::frame::metadata::FrameProduct::reflection_factor(texture.sample_count())
            };
            let view = product.attachment(texture)?;
            opaque
                .additional_colors
                .push(crate::compositor::attachment(resource, &view, false));
            views.push(view);
        }
    }
    Ok(())
}

fn application_source(shader: &str, terrain: bool, enabled: bool) -> String {
    application_metadata(source(shader, terrain), enabled)
}

pub(super) fn application_metadata(shader: String, enabled: bool) -> String {
    if !enabled {
        return shader;
    }
    shader
        .replace(
            "// APPLICATION_FACTOR",
            "published_factor = rd_split_sum(f0, coefficients) * air;",
        )
        .replace(
            "vec4<f32>(published_environment, 1.0)",
            "vec4<f32>(published_environment, published_visibility)",
        )
        .replace(
            "vec4<f32>(published_factor, 1.0)",
            "vec4<f32>(published_factor, published_self_ao)",
        )
}
