//! Authored surface maps share the scene upload table and linear data contract.
use crate::draw::DrawError;
use worldrunner_render_plan::RenderScene;

fn endpoint_clamped(body: &worldrunner_render_plan::SceneWaterBody) -> bool {
    body.material.flow_coordinates.is_some_and(|coordinates| {
        coordinates.mode == worldrunner_render_plan::SceneWaterSeedCoordinates::EndpointClamped
    })
}

pub(crate) fn validate(scene: &RenderScene) -> Result<(), DrawError> {
    if let Some(effects) = scene.effects() {
        for body in effects.water_bodies() {
            for handle in [
                body.material.normal,
                body.material.flow,
                body.material.wind.map(|wind| wind.field.texture()),
            ]
            .into_iter()
            .flatten()
            {
                let index = usize::try_from(handle.raw()).expect("validated texture handle");
                let prepared =
                    crate::texture::prepare_texture(&scene.resources().textures()[index])
                        .map_err(|source| DrawError::Texture { index, source })?;
                if body.material.flow == Some(handle)
                    && endpoint_clamped(body)
                    && (prepared.width() < 2 || prepared.height() < 2)
                {
                    return Err(crate::frame::FrameError::UnsupportedEffect(
                        "endpoint-clamped water seeds require at least 2 samples per axis",
                    )
                    .into());
                }
                if !matches!(
                    prepared.format(),
                    wgpu::TextureFormat::Rg8Unorm
                        | wgpu::TextureFormat::Rgba8Unorm
                        | wgpu::TextureFormat::Bgra8Unorm
                        | wgpu::TextureFormat::Bc5RgUnorm
                ) {
                    return Err(crate::frame::FrameError::UnsupportedEffect(
                        "water maps require linear RG data",
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn entries() -> [wgpu::BindGroupLayoutEntry; 4] {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    [
        texture(7),
        texture(8),
        texture(10),
        wgpu::BindGroupLayoutEntry {
            binding: 9,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

pub(crate) fn inputs(
    scene: &RenderScene,
    views: &[wgpu::TextureView],
) -> Vec<[Option<wgpu::TextureView>; 3]> {
    scene.effects().map_or_else(Vec::new, |effects| {
        effects
            .water_bodies()
            .iter()
            .map(|body| {
                [
                    body.material.normal,
                    body.material.flow,
                    body.material.wind.map(|wind| wind.field.texture()),
                ]
                .map(|handle| {
                    handle.map(|handle| {
                        views[usize::try_from(handle.raw()).expect("validated handle")].clone()
                    })
                })
            })
            .collect()
    })
}
pub(super) fn samplers(
    device: &wgpu::Device,
    bodies: &[worldrunner_render_plan::SceneWaterBody],
) -> Vec<wgpu::Sampler> {
    let mut cache = std::collections::HashMap::new();
    bodies
        .iter()
        .map(|body| {
            let level = body.material.normal_anisotropy;
            cache
                .entry(level)
                .or_insert_with(|| {
                    device.create_sampler(&wgpu::SamplerDescriptor {
                        address_mode_u: wgpu::AddressMode::Repeat,
                        address_mode_v: wgpu::AddressMode::Repeat,
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        mipmap_filter: wgpu::MipmapFilterMode::Linear,
                        anisotropy_clamp: level.level(),
                        ..Default::default()
                    })
                })
                .clone()
        })
        .collect()
}
pub(super) fn binding(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}
pub(crate) fn validate_inputs(
    water: &super::Water,
) -> Result<(), worldrunner_render_post::FrameError> {
    for (body, inputs) in water.settings.iter().zip(&water.surface_inputs) {
        if endpoint_clamped(body)
            && inputs[1].as_ref().is_some_and(|view| {
                let size = view.texture().size();
                size.width < 2 || size.height < 2
            })
        {
            return Err(worldrunner_render_post::FrameError::WaterSettings(
                "endpoint-clamped water seeds require at least 2 samples per axis",
            ));
        }
        if [
            body.material.normal,
            body.material.flow,
            body.material.wind.map(|wind| wind.field.texture()),
        ]
        .iter()
        .zip(inputs)
        .any(|(handle, view)| handle.is_some() && view.is_none())
        {
            return Err(worldrunner_render_post::FrameError::WaterSettings(
                "authored water inputs require a resident scene upload",
            ));
        }
    }
    Ok(())
}
