//! Borrowed, validated texture upload plans. Container decoding belongs to callers.
use thiserror::Error;
use wgpu::{Extent3d, TextureFormat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MipLevel<'a> {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    /// Block rows, not texel rows, for compressed formats.
    pub rows: u32,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct TextureLayer<'a> {
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub mips: &'a [MipLevel<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureKind {
    D2,
    Array,
    Cube,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UploadError {
    #[error("texture extent or layer count is zero")]
    Empty,
    #[error("texture format {0:?} is not a supported color or integer upload format")]
    Format(TextureFormat),
    #[error("texture layout arithmetic overflows")]
    Overflow,
    #[error("mip count {count} is invalid for {width}x{height}")]
    MipCount {
        count: usize,
        width: u32,
        height: u32,
    },
    #[error("base mip {base} is outside the {count}-level chain")]
    BaseMip { base: u32, count: usize },
    #[error("layer {layer} does not match the first layer's format, extent or mip count")]
    LayerMismatch { layer: usize },
    #[error("layer {layer} mip {level} has invalid dimensions or row layout")]
    MipLayout { layer: usize, level: usize },
    #[error("layer {layer} mip {level} needs {expected} bytes, received {actual}")]
    Payload {
        layer: usize,
        level: usize,
        expected: usize,
        actual: usize,
    },
    #[error("a cube needs exactly six square layers")]
    CubeLayers,
    #[error(
        "compressed base extent {width}x{height} must be block-aligned; padding would change sampling"
    )]
    CompressedBase { width: u32, height: u32 },
    #[error("texture exceeds device extent/layer limits")]
    DeviceLimit,
    #[error("texture format requires disabled device features {0:?}")]
    MissingFeatures(wgpu::Features),
}

fn block_layout(format: TextureFormat) -> Result<(u32, u32, u32), UploadError> {
    if format.is_depth_stencil_format() || format.is_multi_planar_format() {
        return Err(UploadError::Format(format));
    }
    let (width, height) = format.block_dimensions();
    let bytes = format
        .block_copy_size(None)
        .ok_or(UploadError::Format(format))?;
    Ok((width, height, bytes))
}

fn layout(
    format: TextureFormat,
    width: u32,
    height: u32,
) -> Result<(u32, u32, usize), UploadError> {
    let (bw, bh, bytes) = block_layout(format)?;
    let pitch = width
        .div_ceil(bw)
        .checked_mul(bytes)
        .ok_or(UploadError::Overflow)?;
    let rows = height.div_ceil(bh);
    let length =
        usize::try_from(u64::from(pitch) * u64::from(rows)).map_err(|_| UploadError::Overflow)?;
    Ok((pitch, rows, length))
}

/// Borrows tightly packed mip data, leaving any trailing container bytes untouched.
pub fn packed_mips(
    format: TextureFormat,
    width: u32,
    height: u32,
    count: u32,
    bytes: &[u8],
) -> Result<Vec<MipLevel<'_>>, UploadError> {
    validate_count(width, height, count as usize)?;
    let mut result = Vec::with_capacity(count as usize);
    let mut offset = 0usize;
    for level in 0..count {
        let width = (width >> level).max(1);
        let height = (height >> level).max(1);
        let (bytes_per_row, rows, length) = layout(format, width, height)?;
        let end = offset.checked_add(length).ok_or(UploadError::Overflow)?;
        let data = bytes.get(offset..end).ok_or(UploadError::Payload {
            layer: 0,
            level: level as usize,
            expected: length,
            actual: bytes.len().saturating_sub(offset),
        })?;
        result.push(MipLevel {
            width,
            height,
            bytes_per_row,
            rows,
            bytes: data,
        });
        offset = end;
    }
    Ok(result)
}

fn validate_count(width: u32, height: u32, count: usize) -> Result<(), UploadError> {
    if width == 0 || height == 0 {
        return Err(UploadError::Empty);
    }
    if count == 0 || count > (u32::BITS - width.max(height).leading_zeros()) as usize {
        return Err(UploadError::MipCount {
            count,
            width,
            height,
        });
    }
    Ok(())
}

#[derive(Debug)]
pub struct TextureUpload<'a> {
    layers: Vec<TextureLayer<'a>>,
    base_mip: u32,
    kind: TextureKind,
}

impl<'a> TextureUpload<'a> {
    pub fn new(
        layers: &[TextureLayer<'a>],
        base_mip: u32,
        kind: TextureKind,
    ) -> Result<Self, UploadError> {
        let first = *layers.first().ok_or(UploadError::Empty)?;
        validate_count(first.width, first.height, first.mips.len())?;
        if base_mip as usize >= first.mips.len() {
            return Err(UploadError::BaseMip {
                base: base_mip,
                count: first.mips.len(),
            });
        }
        if kind == TextureKind::D2 && layers.len() != 1 {
            return Err(UploadError::LayerMismatch { layer: 1 });
        }
        if kind == TextureKind::Cube && (layers.len() != 6 || first.width != first.height) {
            return Err(UploadError::CubeLayers);
        }
        for (layer, image) in layers.iter().enumerate() {
            if image.format != first.format
                || image.width != first.width
                || image.height != first.height
                || image.mips.len() != first.mips.len()
            {
                return Err(UploadError::LayerMismatch { layer });
            }
            for (level, mip) in image.mips.iter().enumerate() {
                let width = (image.width >> level).max(1);
                let height = (image.height >> level).max(1);
                let (pitch, rows, expected) = layout(image.format, width, height)?;
                if (mip.width, mip.height, mip.bytes_per_row, mip.rows)
                    != (width, height, pitch, rows)
                {
                    return Err(UploadError::MipLayout { layer, level });
                }
                if mip.bytes.len() != expected {
                    return Err(UploadError::Payload {
                        layer,
                        level,
                        expected,
                        actual: mip.bytes.len(),
                    });
                }
            }
        }
        let width = (first.width >> base_mip).max(1);
        let height = (first.height >> base_mip).max(1);
        let (bw, bh, _) = block_layout(first.format)?;
        if !width.is_multiple_of(bw) || !height.is_multiple_of(bh) {
            return Err(UploadError::CompressedBase { width, height });
        }
        Ok(Self {
            layers: layers.to_vec(),
            base_mip,
            kind,
        })
    }

    pub fn kind(&self) -> TextureKind {
        self.kind
    }

    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        extra_usage: wgpu::TextureUsages,
    ) -> Result<wgpu::Texture, UploadError> {
        let first = self.layers[0];
        let width = (first.width >> self.base_mip).max(1);
        let height = (first.height >> self.base_mip).max(1);
        let layer_count = u32::try_from(self.layers.len()).map_err(|_| UploadError::Overflow)?;
        let limits = device.limits();
        if width.max(height) > limits.max_texture_dimension_2d
            || layer_count > limits.max_texture_array_layers
        {
            return Err(UploadError::DeviceLimit);
        }
        let missing = first
            .format
            .required_features()
            .difference(device.features());
        if !missing.is_empty() {
            return Err(UploadError::MissingFeatures(missing));
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: layer_count,
            },
            mip_level_count: u32::try_from(first.mips.len()).map_err(|_| UploadError::Overflow)?
                - self.base_mip,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: first.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | extra_usage,
            view_formats: &[],
        });
        self.write_mips(queue, &texture)?;
        Ok(texture)
    }

    fn write_mips(&self, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Result<(), UploadError> {
        let (bw, bh) = texture.format().block_dimensions();
        for (layer, image) in self.layers.iter().enumerate() {
            for (level, mip) in image.mips.iter().enumerate().skip(self.base_mip as usize) {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: u32::try_from(level).map_err(|_| UploadError::Overflow)?
                            - self.base_mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: u32::try_from(layer).map_err(|_| UploadError::Overflow)?,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    mip.bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip.bytes_per_row),
                        rows_per_image: Some(mip.rows),
                    },
                    Extent3d {
                        width: mip.width.next_multiple_of(bw),
                        height: mip.height.next_multiple_of(bh),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        Ok(())
    }
}
