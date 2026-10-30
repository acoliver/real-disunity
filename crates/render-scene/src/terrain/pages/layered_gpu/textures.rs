use super::{LayeredError, LayeredGpuError, LayeredGpuSource};
use crate::texture::{OwnedPreparedTexture, prepare_texture};
use worldrunner_render_plan::SceneTexture;
use worldrunner_render_post::contracts::{
    BufferView, CheckedSampler, ContractError, ResourceLifetime, ResourceRegistry, SamplerBinding,
    Semantic, TextureBinding, TextureBindingKind, TextureRange,
};

// Terrain, shadow and screen-space bindings reserve nine fragment texture slots.
pub(crate) const ARRAY_LIMIT: u32 = 7;

#[derive(Debug)]
pub(crate) struct ResidentSource {
    pub buffers: Vec<wgpu::Buffer>,
    textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    pub samplers: [wgpu::Sampler; 2],
    checked_samplers: [CheckedSampler; 2],
}

pub(super) fn image(
    result: &mut LayeredGpuSource,
    texture: &SceneTexture,
) -> Result<(), LayeredError> {
    let prepared = prepare_texture(texture).map_err(|_| LayeredError::Texture)?;
    validate_format(prepared.format())?;
    let (width, height) = prepared.format().block_dimensions();
    if !prepared.width().is_multiple_of(width) || !prepared.height().is_multiple_of(height) {
        return Err(LayeredError::Texture);
    }
    let count = u32::try_from(prepared.mips().len()).map_err(|_| LayeredError::Texture)?;
    let expected = u32::BITS - prepared.width().max(prepared.height()).leading_zeros();
    if count != 1 && count != expected {
        return Err(LayeredError::IncompleteMipChain {
            texture: u32::try_from(result.images.len() / 16).map_err(|_| LayeredError::Texture)?,
            declared: count,
            expected,
        });
    }
    let group = result
        .arrays
        .iter()
        .position(|images| {
            let first = images[0].as_prepared();
            (
                first.format(),
                first.width(),
                first.height(),
                first.mips().len(),
            ) == (
                prepared.format(),
                prepared.width(),
                prepared.height(),
                prepared.mips().len(),
            )
        })
        .unwrap_or(result.arrays.len());
    if group == result.arrays.len() {
        if group >= ARRAY_LIMIT as usize {
            return Err(LayeredError::Texture);
        }
        result.arrays.push(Vec::new());
    }
    let layer = u32::try_from(result.arrays[group].len()).map_err(|_| LayeredError::Texture)?;
    super::source::words(
        &mut result.images,
        [
            u32::try_from(group).map_err(|_| LayeredError::Texture)?,
            layer,
            prepared.width(),
            prepared.height(),
        ],
    );
    result.arrays[group].push(prepared.into_owned());
    Ok(())
}

fn validate_format(format: wgpu::TextureFormat) -> Result<(), LayeredError> {
    if !matches!(
        format,
        wgpu::TextureFormat::R8Unorm
            | wgpu::TextureFormat::Rg8Unorm
            | wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::Bc1RgbaUnorm
            | wgpu::TextureFormat::Bc1RgbaUnormSrgb
            | wgpu::TextureFormat::Bc3RgbaUnorm
            | wgpu::TextureFormat::Bc3RgbaUnormSrgb
            | wgpu::TextureFormat::Bc4RUnorm
            | wgpu::TextureFormat::Bc5RgUnorm
    ) {
        return Err(LayeredError::Texture);
    }
    Ok(())
}

pub(super) fn payload_bytes(arrays: &[Vec<OwnedPreparedTexture>]) -> u64 {
    arrays
        .iter()
        .flatten()
        .map(|image| {
            image
                .as_prepared()
                .mips()
                .iter()
                .map(|mip| mip.bytes.len() as u64)
                .sum::<u64>()
        })
        .sum()
}

fn sampler(
    repeat: bool,
    anisotropy: worldrunner_render_plan::TextureAnisotropy,
) -> wgpu::SamplerDescriptor<'static> {
    let address = if repeat {
        wgpu::AddressMode::Repeat
    } else {
        wgpu::AddressMode::ClampToEdge
    };
    wgpu::SamplerDescriptor {
        anisotropy_clamp: anisotropy.level(),
        address_mode_u: address,
        address_mode_v: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    }
}

impl LayeredGpuSource {
    pub(crate) fn allocations(&self) -> worldrunner_render_gpu::configuration::AllocationSet {
        let mut set = worldrunner_render_gpu::configuration::AllocationSet::default();
        for (name, bytes) in [
            ("layered parameters", &self.parameters),
            ("layered grid", &self.grid),
            ("layered image map", &self.images),
        ] {
            set.shared_buffer(name, bytes.len() as u64, 1);
        }
        let names = [
            "layer array 0",
            "layer array 1",
            "layer array 2",
            "layer array 3",
            "layer array 4",
            "layer array 5",
            "layer array 6",
            "layer array 7",
            "layer array 8",
            "layer array 9",
        ];
        for (images, name) in self.arrays.iter().zip(names) {
            let first = images[0].as_prepared();
            set.shared_texture(
                name,
                [
                    first.width(),
                    first.height(),
                    u32::try_from(images.len()).expect("layer count"),
                ],
                first.format(),
                [u32::try_from(first.mips().len()).expect("mip count"), 1],
                1,
            );
        }
        set
    }

    pub(crate) fn resident(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<std::sync::Arc<ResidentSource>, LayeredGpuError> {
        let mut residents = self.resident.lock().expect("source residency lock");
        if let Some(resident) = residents.get(device) {
            return Ok(resident.clone());
        }
        self.validate_device(device)?;
        let resident = std::sync::Arc::new(self.upload_source(device, queue)?);
        residents.insert(device.clone(), resident.clone());
        Ok(resident)
    }

    fn upload_source(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<ResidentSource, LayeredGpuError> {
        let buffers = [&self.parameters, &self.grid, &self.images]
            .into_iter()
            .map(|bytes| {
                crate::upload::upload_buffer(
                    device,
                    queue,
                    "resident layered metadata",
                    wgpu::BufferUsages::STORAGE,
                    bytes,
                )
            })
            .collect();
        let mut textures = Vec::new();
        for images in &self.arrays {
            let prepared: Vec<_> = images
                .iter()
                .map(OwnedPreparedTexture::as_prepared)
                .collect();
            textures.push(crate::upload::upload_palette_array(
                device,
                queue,
                "resident layered textures",
                &prepared,
            )?);
        }
        let views = (0..ARRAY_LIMIT as usize)
            .map(|index| {
                textures[index.min(textures.len() - 1)].create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                })
            })
            .collect();
        let descriptors = [
            sampler(true, self.anisotropy),
            sampler(false, self.anisotropy),
        ];
        Ok(ResidentSource {
            buffers,
            textures,
            views,
            samplers: descriptors
                .each_ref()
                .map(|descriptor| device.create_sampler(descriptor)),
            checked_samplers: descriptors
                .each_ref()
                .map(|descriptor| CheckedSampler::new(device, descriptor)),
        })
    }

    pub(super) fn import_source(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        registry: &mut ResourceRegistry,
    ) -> Result<Vec<BufferView>, LayeredGpuError> {
        self.resident(device, queue)?
            .buffers
            .iter()
            .map(|buffer| {
                let key =
                    registry.import_buffer(buffer, ResourceLifetime::ImportedReadOnly, Some(1))?;
                Ok(registry.buffer_view(key, 0..buffer.size(), 1)?)
            })
            .collect()
    }

    pub(super) fn texture_bindings(
        &self,
        device: &wgpu::Device,
        registry: &mut ResourceRegistry,
    ) -> Result<(Vec<TextureBinding>, Vec<SamplerBinding>), ContractError> {
        let resident = self
            .resident
            .lock()
            .expect("source residency lock")
            .get(device)
            .expect("source uploaded before composition")
            .clone();
        let textures = (0..ARRAY_LIMIT)
            .map(|index| {
                let texture = &resident.textures[(index as usize).min(resident.textures.len() - 1)];
                let key = registry.import(
                    texture,
                    ResourceLifetime::ImportedReadOnly,
                    Semantic::Unspecified,
                    Some(1),
                )?;
                Ok(TextureBinding {
                    binding: 5 + index,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    view: registry.view_dimension(
                        key,
                        TextureRange {
                            mips: 0..texture.mip_level_count(),
                            layers: 0..texture.depth_or_array_layers(),
                            aspect: wgpu::TextureAspect::All,
                        },
                        1,
                        wgpu::TextureViewDimension::D2Array,
                    )?,
                    samples: 1,
                    semantic: Semantic::Unspecified,
                    kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Float {
                        filterable: true,
                    }),
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        let samplers = resident
            .checked_samplers
            .iter()
            .enumerate()
            .map(|(index, sampler)| SamplerBinding {
                binding: 15 + u32::try_from(index).expect("two samplers"),
                visibility: wgpu::ShaderStages::COMPUTE,
                sampler: sampler.clone(),
            })
            .collect();
        Ok((textures, samplers))
    }
}

pub(crate) fn draw_layout() -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries: Vec<_> = (0..ARRAY_LIMIT)
        .map(|index| wgpu::BindGroupLayoutEntry {
            binding: 11 + index,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    entries.extend((21..23).map(|binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }));
    entries
}
