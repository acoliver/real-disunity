use crate::texture::PreparedTexture;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferDescriptor, BufferUsages, Device, Extent3d, FilterMode, Origin3d, Queue, Sampler,
    SamplerDescriptor, ShaderStages, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureViewDimension,
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
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX_FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
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
        visibility: ShaderStages::FRAGMENT,
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
    entries.push(BindGroupLayoutEntry {
        binding: 6,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
    let mut entries = Vec::with_capacity(6);
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
    for binding in 2..=4 {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        });
    }
    entries.push(BindGroupLayoutEntry {
        binding: 5,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
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
) -> Texture {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("terrain-type-grid"),
        size: Extent3d {
            width: columns,
            height: rows,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Uint,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        indices,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(columns),
            rows_per_image: Some(rows),
        },
        Extent3d {
            width: columns,
            height: rows,
            depth_or_array_layers: 1,
        },
    );
    texture
}

pub fn upload_palette_array(
    device: &Device,
    queue: &Queue,
    label: &str,
    layers: &[PreparedTexture],
) -> Option<Texture> {
    let first = layers.first()?;
    let layer_count = u32::try_from(layers.len()).ok()?;
    let mip_level_count = u32::try_from(first.mips().len()).unwrap_or(1).max(1);
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width: first.width(),
            height: first.height(),
            depth_or_array_layers: layer_count,
        },
        mip_level_count,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: first.format(),
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (block_width, block_height) = first.format().block_dimensions();
    for (layer, prepared) in layers.iter().enumerate() {
        let layer_index = u32::try_from(layer).unwrap_or(0);
        for (level, mip) in prepared.mips().iter().enumerate() {
            let mip_level = u32::try_from(level).unwrap_or(0);
            if mip_level >= mip_level_count {
                break;
            }
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: layer_index,
                    },
                    aspect: TextureAspect::All,
                },
                mip.bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.bytes_per_row),
                    rows_per_image: Some(mip.rows),
                },
                Extent3d {
                    width: whole_blocks(mip.width, block_width),
                    height: whole_blocks(mip.height, block_height),
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    Some(texture)
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

/// Rounds a mip extent up to whole blocks.
///
/// Below a certain level a block-compressed mip chain stops shrinking in
/// texels and starts padding: the 2x2 and 1x1 levels of a BC format each still
/// occupy one whole 4x4 block. A copy extent has to describe that physical
/// size, because the logical extent a mip level reports is neither a multiple
/// of the block width nor, once rounded up, inside the level's own bounds --
/// so the driver rejects it. The failure would appear only at the tail of
/// every chain, on the two levels that are hardest to see.
fn whole_blocks(extent: u32, block: u32) -> u32 {
    extent.next_multiple_of(block)
}

/// Uploads a prepared texture, copying every mip level block for block.
pub fn upload_texture(device: &Device, queue: &Queue, prepared: &PreparedTexture) -> Texture {
    let mip_level_count = u32::try_from(prepared.mips().len()).unwrap_or(1).max(1);
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("material-texture"),
        size: Extent3d {
            width: prepared.width(),
            height: prepared.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: prepared.format(),
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (block_width, block_height) = prepared.format().block_dimensions();
    for (level, mip) in prepared.mips().iter().enumerate() {
        let mip_level = u32::try_from(level).unwrap_or(0);
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            mip.bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.bytes_per_row),
                rows_per_image: Some(mip.rows),
            },
            Extent3d {
                width: whole_blocks(mip.width, block_width),
                height: whole_blocks(mip.height, block_height),
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

/// Creates the neutral texture bound where a material omits an optional map.
pub fn placeholder_texture(device: &Device, queue: &Queue) -> Texture {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("placeholder"),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &PLACEHOLDER_TEXEL,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture
}

/// Uploads already-encoded bytes into a buffer of the requested usage.
///
/// Every caller in this crate encodes its own bytes, because the ban on
/// unsafe code rules out casting a struct to a byte slice. That keeps the
/// layout explicit in Rust and testable without a GPU.
pub fn upload_buffer(
    device: &Device,
    queue: &Queue,
    label: &str,
    usage: BufferUsages,
    bytes: &[u8],
) -> Buffer {
    let size = u64::try_from(bytes.len()).unwrap_or(0);
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: size.max(wgpu::COPY_BUFFER_ALIGNMENT),
        usage: usage | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !bytes.is_empty() {
        queue.write_buffer(&buffer, 0, bytes);
    }
    buffer
}

#[cfg(test)]
mod tests;
