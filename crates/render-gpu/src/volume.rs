//! Borrowed, validated uploads of 3D textures with a mip chain. A level of a volume has its own
//! depth, so a volume is not the layers of [`TextureUpload`](crate::texture::TextureUpload): it is
//! levels, each holding its slices. Block-compressed formats are not volumes here, as a device
//! samples them in 3D only with a feature this crate does not enable.
use wgpu::Extent3d;

use crate::texture::{UploadError, layout};

/// One level of a volume: `depth` slices of `rows` rows of `bytes_per_row` bytes, tightly packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeLevel<'a> {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub bytes_per_row: u32,
    pub rows: u32,
    pub bytes: &'a [u8],
}

fn halved(side: u32, level: usize) -> u32 {
    u32::try_from(level)
        .ok()
        .and_then(|shift| side.checked_shr(shift))
        .unwrap_or(0)
        .max(1)
}

fn validate_count(size: [u32; 3], count: usize) -> Result<(), UploadError> {
    if size.contains(&0) {
        return Err(UploadError::Empty);
    }
    let longest = size.into_iter().max().unwrap_or(1);
    if count == 0 || count > (u32::BITS - longest.leading_zeros()) as usize {
        return Err(UploadError::MipCount {
            count,
            width: size[0],
            height: size[1],
        });
    }
    Ok(())
}

/// Borrows tightly packed levels of a `size` volume, each level's slices one after the other and
/// the levels one after the other from the base. Trailing bytes are left untouched. A level is as
/// wide, as high and as deep as the base shifted down, and at least one texel.
///
/// # Errors
/// Fails when an extent is zero, the level count is zero or more than the size has, the format is
/// not an uncompressed color format, or `bytes` is shorter than the chain.
pub fn packed_volume_levels(
    format: wgpu::TextureFormat,
    size: [u32; 3],
    count: u32,
    bytes: &[u8],
) -> Result<Vec<VolumeLevel<'_>>, UploadError> {
    validate_count(size, count as usize)?;
    if format.is_compressed() {
        return Err(UploadError::Format(format));
    }
    let mut levels = Vec::with_capacity(count as usize);
    let mut offset = 0usize;
    for level in 0..count as usize {
        let [width, height, depth] = size.map(|side| halved(side, level));
        let (bytes_per_row, rows, slice) = layout(format, width, height)?;
        let length = slice
            .checked_mul(depth as usize)
            .ok_or(UploadError::Overflow)?;
        let end = offset.checked_add(length).ok_or(UploadError::Overflow)?;
        let data = bytes.get(offset..end).ok_or(UploadError::Payload {
            layer: 0,
            level,
            expected: length,
            actual: bytes.len().saturating_sub(offset),
        })?;
        levels.push(VolumeLevel {
            width,
            height,
            depth,
            bytes_per_row,
            rows,
            bytes: data,
        });
        offset = end;
    }
    Ok(levels)
}

/// A volume of one format, ready to upload.
#[derive(Debug)]
pub struct VolumeUpload<'a> {
    format: wgpu::TextureFormat,
    levels: Vec<VolumeLevel<'a>>,
}

impl<'a> VolumeUpload<'a> {
    /// Checks `levels` as the chain of a volume whose base level is the first.
    ///
    /// # Errors
    /// Fails when there is no level or too many for the base extent, the format is block
    /// compressed or not a color format, or a level's extent, row layout or byte count is not
    /// the one its place in the chain gives.
    pub fn new(
        format: wgpu::TextureFormat,
        levels: &[VolumeLevel<'a>],
    ) -> Result<Self, UploadError> {
        let base = *levels.first().ok_or(UploadError::Empty)?;
        validate_count([base.width, base.height, base.depth], levels.len())?;
        if format.is_compressed() {
            return Err(UploadError::Format(format));
        }
        for (level, image) in levels.iter().enumerate() {
            let expected = [base.width, base.height, base.depth].map(|side| halved(side, level));
            let (pitch, rows, slice) = layout(format, expected[0], expected[1])?;
            let shape = [image.width, image.height, image.depth];
            if shape != expected || (image.bytes_per_row, image.rows) != (pitch, rows) {
                return Err(UploadError::MipLayout { layer: 0, level });
            }
            let length = slice * expected[2] as usize;
            if image.bytes.len() != length {
                return Err(UploadError::Payload {
                    layer: 0,
                    level,
                    expected: length,
                    actual: image.bytes.len(),
                });
            }
        }
        Ok(Self {
            format,
            levels: levels.to_vec(),
        })
    }

    /// Creates the texture and writes every level.
    ///
    /// # Errors
    /// Fails when the base extent is beyond the device's 3D limit or the format needs a device
    /// feature that is not enabled.
    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        extra_usage: wgpu::TextureUsages,
    ) -> Result<wgpu::Texture, UploadError> {
        let base = self.levels[0];
        if base.width.max(base.height).max(base.depth) > device.limits().max_texture_dimension_3d {
            return Err(UploadError::DeviceLimit);
        }
        let missing = self
            .format
            .required_features()
            .difference(device.features());
        if !missing.is_empty() {
            return Err(UploadError::MissingFeatures(missing));
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: base.width,
                height: base.height,
                depth_or_array_layers: base.depth,
            },
            mip_level_count: u32::try_from(self.levels.len()).map_err(|_| UploadError::Overflow)?,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | extra_usage,
            view_formats: &[],
        });
        for (level, image) in self.levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: u32::try_from(level).map_err(|_| UploadError::Overflow)?,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                image.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.bytes_per_row),
                    rows_per_image: Some(image.rows),
                },
                Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: image.depth,
                },
            );
        }
        Ok(texture)
    }
}
