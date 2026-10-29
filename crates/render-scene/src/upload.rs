use crate::texture::PreparedTexture;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Device,
    FilterMode, Queue, Sampler, SamplerDescriptor, ShaderStages, Texture, TextureFormat,
    TextureSampleType, TextureUsages, TextureViewDimension,
};
use worldrunner_render_gpu::texture::{
    TextureKind, TextureLayer, TextureUpload, UploadError, packed_mips,
};

/// Bind group index carrying per-frame state.
pub const FRAME_GROUP: u32 = 0;

/// Bind group index carrying per-material state.
pub const MATERIAL_GROUP: u32 = 1;

/// Bind group index carrying the terrain splat state.
///
/// Terrain occupies the same slot as a material because it is the alternative
/// to one, not an addition: a terrain fragment chooses its material from the
/// type grid instead of being told which one the draw carries.
pub const TERRAIN_GROUP: u32 = 1;

/// Number of optional textures a material may omit.
pub const OPTIONAL_TEXTURE_COUNT: usize = 4;

/// The neutral texel bound where a material omits an optional texture.
const PLACEHOLDER_TEXEL: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// Builds the layout for per-frame state, matching `@group(0)` in the shader.
pub fn frame_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("frame"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            decal_texture_entry(2),
            decal_texture_entry(4),
            decal_texture_entry(5),
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // The previous-frame block. Binding 1 already carries the decal list, so this
            // takes the next free slot. A shader that never names it is unaffected.
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        crate::frame::temporal::PREVIOUS_FRAME_BYTES as u64,
                    ),
                },
                count: None,
            },
        ],
    })
}

fn decal_texture_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

/// Builds the layout for per-material state, matching `@group(1)`.
///
/// The five texture bindings are declared in the order the shader declares
/// them: albedo, normal, packed roughness/metallic, ambient occlusion and
/// self-illumination.
pub fn material_bind_group_layout(device: &Device) -> BindGroupLayout {
    let mut entries = Vec::with_capacity(7);
    entries.push(BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    for binding in 1..=5 {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    for (binding, visibility) in [
        (8, ShaderStages::FRAGMENT),
        (9, ShaderStages::VERTEX),
        (10, ShaderStages::VERTEX),
    ] {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    entries.push(BindGroupLayoutEntry {
        binding: 6,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    entries.push(BindGroupLayoutEntry {
        binding: 7,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("material"),
        entries: &entries,
    })
}

/// Builds the layout for terrain splat state, matching `@group(1)` of the
/// terrain shader.
///
/// The type grid is declared as an unfilterable `uint` texture and is read with
/// `textureLoad` rather than sampled. That is not a limitation being worked
/// around: filtering a material identifier would interpolate between two
/// unrelated palette indices and produce a third that names a different
/// material entirely. The shader blends coverage masks derived from exact
/// identifier loads rather than interpolating identifiers.
///
/// The three palette maps are arrays rather than separate bindings because the
/// fragment shader cannot know in advance which materials a given fragment will
/// need to blend. Every authored terrain texture shares one size per map kind,
/// so the layers stack without resampling.
pub fn terrain_bind_group_layout(device: &Device) -> BindGroupLayout {
    terrain_layout(device, false, false)
}

pub(crate) fn terrain_layout(device: &Device, cached: bool, layered: bool) -> BindGroupLayout {
    let mut entries = vec![BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    entries.push(BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Uint,
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    if !layered {
        entries.extend((2..=4).map(decal_texture_entry));
    }
    entries.push(BindGroupLayoutEntry {
        binding: 5,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    for binding in 8..if layered { 11 } else { 8 } {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    if layered {
        entries.extend(crate::terrain::pages::layered_gpu::textures::draw_layout());
    }
    if cached {
        install_cached_entries(&mut entries);
    }
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("terrain"),
        entries: &entries,
    })
}

/// Uploads the terrain material-index grid as an unfilterable `uint` texture.
///
/// One byte per cell is copied verbatim. The grid is the authority for which
/// material a fragment stands on, so it is never decimated on the way to the
/// GPU: decimating it here would reintroduce exactly the loss that per-fragment
/// selection exists to avoid.
pub fn upload_type_grid(
    device: &Device,
    queue: &Queue,
    columns: u32,
    rows: u32,
    indices: &[u8],
) -> Result<Texture, UploadError> {
    let mips = packed_mips(TextureFormat::R8Uint, columns, rows, 1, indices)?;
    TextureUpload::new(
        &[TextureLayer {
            format: TextureFormat::R8Uint,
            width: columns,
            height: rows,
            mips: &mips,
        }],
        0,
        TextureKind::D2,
    )?
    .upload(device, queue, "terrain-type-grid", TextureUsages::empty())
}

pub fn upload_palette_array(
    device: &Device,
    queue: &Queue,
    label: &str,
    layers: &[PreparedTexture],
) -> Result<Texture, UploadError> {
    let layers: Vec<_> = layers.iter().map(texture_layer).collect();
    TextureUpload::new(&layers, 0, TextureKind::Array)?.upload(
        device,
        queue,
        label,
        TextureUsages::empty(),
    )
}

fn texture_layer<'a>(prepared: &'a PreparedTexture<'a>) -> TextureLayer<'a> {
    TextureLayer {
        format: prepared.format(),
        width: prepared.width(),
        height: prepared.height(),
        mips: prepared.mips(),
    }
}

pub fn material_sampler(device: &Device) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("material"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..SamplerDescriptor::default()
    })
}

/// Uploads all validated mip levels through the common resource implementation.
pub fn upload_texture(
    device: &Device,
    queue: &Queue,
    prepared: &PreparedTexture,
) -> Result<Texture, UploadError> {
    TextureUpload::new(&[texture_layer(prepared)], 0, TextureKind::D2)?.upload(
        device,
        queue,
        "material-texture",
        TextureUsages::empty(),
    )
}

/// Creates the neutral texture bound where a material omits an optional map.
pub fn placeholder_texture(device: &Device, queue: &Queue) -> Texture {
    let mips = packed_mips(TextureFormat::Rgba8Unorm, 1, 1, 1, &PLACEHOLDER_TEXEL)
        .expect("constant texel layout");
    TextureUpload::new(
        &[TextureLayer {
            format: TextureFormat::Rgba8Unorm,
            width: 1,
            height: 1,
            mips: &mips,
        }],
        0,
        TextureKind::D2,
    )
    .expect("constant texel layout")
    .upload(device, queue, "placeholder", TextureUsages::empty())
    .expect("constant texture supported")
}

/// Uploads already-encoded bytes into a buffer of the requested usage.
///
/// Every caller in this crate encodes its own bytes, because the ban on
/// unsafe code rules out casting a struct to a byte slice. That keeps the
/// layout explicit in Rust and testable without a GPU.
pub use worldrunner_render_gpu::resource::upload_buffer;

#[cfg(test)]
mod tests;

fn cached_terrain_entries() -> Vec<BindGroupLayoutEntry> {
    vec![
        BindGroupLayoutEntry {
            binding: 6,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Uint,
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]
}

/// Builds the shared material bindings, including the scene contact heightfield.
pub(crate) fn material_group(
    device: &Device,
    layout: &BindGroupLayout,
    uniform: &wgpu::Buffer,
    textures: &[&wgpu::TextureView; 5],
    sampler: &Sampler,
    heights: &wgpu::Buffer,
) -> wgpu::BindGroup {
    extended_material_group(
        device,
        layout,
        uniform,
        MaterialTextureBindings {
            maps: *textures,
            extra: [textures[0]; 3],
        },
        sampler,
        heights,
    )
}

#[derive(Clone, Copy)]
pub(crate) struct MaterialTextureBindings<'a> {
    pub maps: [&'a wgpu::TextureView; 5],
    pub extra: [&'a wgpu::TextureView; 3],
}

pub(crate) fn extended_material_group(
    device: &Device,
    layout: &BindGroupLayout,
    uniform: &wgpu::Buffer,
    textures: MaterialTextureBindings<'_>,
    sampler: &Sampler,
    heights: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform.as_entire_binding(),
    }];
    for (index, view) in textures.maps.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: u32::try_from(index + 1).expect("five textures"),
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: 6,
        resource: wgpu::BindingResource::Sampler(sampler),
    });
    entries.push(wgpu::BindGroupEntry {
        binding: 7,
        resource: heights.as_entire_binding(),
    });
    for (binding, view) in [8, 9, 10].into_iter().zip(textures.extra) {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material"),
        layout,
        entries: &entries,
    })
}

fn install_cached_entries(entries: &mut Vec<BindGroupLayoutEntry>) {
    entries.retain(|entry| entry.binding != 1);
    entries.extend(cached_terrain_entries());
}
