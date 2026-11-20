//! Sampled textures a game declares to its [`GameResources`](super::GameResources): what a
//! declaration may describe, the layout of the texels it carries, and their upload through the
//! same validated plan the scene's textures use (render-gpu `TextureUpload`).
use super::resources::GameResourceError;
use std::hash::{Hash, Hasher};
use worldrunner_render_gpu::texture::{
    MipLevel, TextureKind, TextureLayer, TextureUpload, level_layout, packed_mips,
};

/// A sampled texture a game declares: a mip chain of one or more layers, or a 3D texture.
#[derive(Clone, Copy, Debug)]
pub struct GameTextureDeclaration<'a> {
    /// Names the texture in errors, labels and the census, and a shared texture among the
    /// factories of a generation.
    pub label: &'a str,
    pub format: wgpu::TextureFormat,
    /// Width, height, and array layers (for `D2` and `D2Array`) or depth (for `D3`).
    pub size: [u32; 3],
    /// How the texture is sampled: `D2` (one layer), `D2Array` or `D3`.
    pub dimension: wgpu::TextureViewDimension,
    /// Mip levels, at least one: level `m` is `max(1, width >> m)` by `max(1, height >> m)`.
    pub mip_levels: u32,
    /// Every level's texels in the order [`Self::levels`] lists them: layer after layer, each
    /// layer's chain finest first, each level tightly packed rows of texels, or of blocks for a
    /// block-compressed format. `None` leaves the texture zeroed, which is the only choice for a
    /// depth format and for a 3D texture deeper than one slice.
    pub texels: Option<&'a [u8]>,
}

/// Where one level of one layer sits in [`GameTextureDeclaration::texels`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameTextureLevel {
    pub layer: u32,
    pub mip: u32,
    /// The level's extent in texels.
    pub width: u32,
    pub height: u32,
    /// Bytes of one row of texels, or of blocks: whole blocks times the block's bytes.
    pub bytes_per_row: u32,
    /// Rows of texels, or of blocks.
    pub rows: u32,
    /// Where the level starts in the texels, and its bytes.
    pub offset: u64,
    pub len: u64,
}

impl GameTextureDeclaration<'_> {
    /// The levels the texels hold, in their order.
    ///
    /// # Errors
    /// [`GameResourceError::Declaration`] for a declaration that cannot hold texels (a depth
    /// format, a 3D texture deeper than one slice) or whose extent or mip count is invalid.
    pub fn levels(&self) -> Result<Vec<GameTextureLevel>, GameResourceError> {
        self.descriptor()?;
        self.packed_levels()
    }

    /// [`Self::levels`] without validating the declaration first.
    fn packed_levels(&self) -> Result<Vec<GameTextureLevel>, GameResourceError> {
        if self.format.is_depth_stencil_format() {
            return Err(refuse(
                self.label,
                "a depth texture is declared without texels",
            ));
        }
        let [width, height, layers] = self.size;
        let layers = match self.dimension {
            wgpu::TextureViewDimension::D3 if layers != 1 => {
                return Err(refuse(
                    self.label,
                    "a 3D texture deeper than one slice is declared without texels",
                ));
            }
            wgpu::TextureViewDimension::D3 => 1,
            _ => layers,
        };
        let mut levels = Vec::new();
        let mut offset = 0_u64;
        for layer in 0..layers {
            for mip in 0..self.mip_levels {
                let (level_width, level_height) = ((width >> mip).max(1), (height >> mip).max(1));
                let (bytes_per_row, rows, len) =
                    level_layout(self.format, level_width, level_height)
                        .map_err(|error| refuse(self.label, &error.to_string()))?;
                let len = len as u64;
                levels.push(GameTextureLevel {
                    layer,
                    mip,
                    width: level_width,
                    height: level_height,
                    bytes_per_row,
                    rows,
                    offset,
                    len,
                });
                offset += len;
            }
        }
        Ok(levels)
    }

    /// The descriptor of the texture, refused when the declaration cannot be created.
    pub(super) fn descriptor(&self) -> Result<wgpu::TextureDescriptor<'_>, GameResourceError> {
        let label = self.label;
        let [width, height, layers] = self.size;
        let volume = self.dimension == wgpu::TextureViewDimension::D3;
        let dimension = match self.dimension {
            wgpu::TextureViewDimension::D2 if layers == 1 => wgpu::TextureDimension::D2,
            wgpu::TextureViewDimension::D2Array => wgpu::TextureDimension::D2,
            wgpu::TextureViewDimension::D3 => wgpu::TextureDimension::D3,
            _ => {
                return Err(refuse(
                    label,
                    "a declared texture is sampled as D2 (one layer), D2Array or D3",
                ));
            }
        };
        if width == 0 || height == 0 || layers == 0 {
            return Err(refuse(label, "the extent is empty"));
        }
        let (block_width, block_height) = self.format.block_dimensions();
        if !width.is_multiple_of(block_width) || !height.is_multiple_of(block_height) {
            return Err(refuse(
                label,
                "the extent is not whole blocks of the format",
            ));
        }
        if volume && self.format.is_depth_stencil_format() {
            return Err(refuse(label, "a depth texture is not 3D"));
        }
        if volume && self.format.is_compressed() {
            return Err(refuse(label, "a 3D texture is not block-compressed"));
        }
        let longest = if volume {
            width.max(height).max(layers)
        } else {
            width.max(height)
        };
        let most = u32::BITS - longest.leading_zeros();
        if self.mip_levels == 0 || self.mip_levels > most {
            return Err(refuse(
                label,
                &format!(
                    "{} mip levels, a {width} x {height} texture has 1 to {most}",
                    self.mip_levels
                ),
            ));
        }
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING;
        if let Some(texels) = self.texels {
            let needed: u64 = self.packed_levels()?.iter().map(|level| level.len).sum();
            if texels.len() as u64 != needed {
                return Err(refuse(
                    label,
                    &format!(
                        "the texels are {} bytes, the {} levels {needed}",
                        texels.len(),
                        self.mip_levels
                    ),
                ));
            }
            usage |= wgpu::TextureUsages::COPY_DST;
        }
        Ok(wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: self.mip_levels,
            sample_count: 1,
            dimension,
            format: self.format,
            usage,
            view_formats: &[],
        })
    }

    /// What two declarations of one shared texture must agree on: everything they declare,
    /// with the texels by digest.
    pub(super) fn identity(&self) -> TextureIdentity {
        TextureIdentity {
            format: self.format,
            size: self.size,
            dimension: self.dimension,
            mip_levels: self.mip_levels,
            texels: self.texels.map(|texels| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                texels.hash(&mut hasher);
                (texels.len(), hasher.finish())
            }),
        }
    }
}

/// See [`GameTextureDeclaration::identity`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextureIdentity {
    format: wgpu::TextureFormat,
    size: [u32; 3],
    dimension: wgpu::TextureViewDimension,
    mip_levels: u32,
    texels: Option<(usize, u64)>,
}

/// Refuses a texture past the device's `limits` (its extent, or its layers), which wgpu would
/// otherwise report as a validation panic. [`unsampleable`] refuses a format the device lacks.
pub(super) fn beyond_limits(
    limits: &wgpu::Limits,
    descriptor: &wgpu::TextureDescriptor<'_>,
) -> Result<(), GameResourceError> {
    let label = descriptor.label.unwrap_or_default();
    let wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: depth,
    } = descriptor.size;
    let extent = width.max(height);
    let (extent_fits, layers_fit) = match descriptor.dimension {
        wgpu::TextureDimension::D3 => (extent.max(depth) <= limits.max_texture_dimension_3d, true),
        _ => (
            extent <= limits.max_texture_dimension_2d,
            depth <= limits.max_texture_array_layers,
        ),
    };
    if !extent_fits {
        return Err(refuse(
            label,
            "the extent is beyond the device's texture limit",
        ));
    }
    if !layers_fit {
        return Err(refuse(
            label,
            "there are more layers than the device allows",
        ));
    }
    Ok(())
}

/// Why a device with `features` cannot sample `format`, if it cannot.
pub(super) fn unsampleable(
    format: wgpu::TextureFormat,
    features: wgpu::Features,
) -> Option<String> {
    let missing = format.required_features().difference(features);
    if !missing.is_empty() {
        return Some(format!(
            "the device cannot sample {format:?} without features {missing:?}"
        ));
    }
    let allowed = format.guaranteed_format_features(features).allowed_usages;
    (!allowed.contains(wgpu::TextureUsages::TEXTURE_BINDING))
        .then(|| format!("the device cannot sample {format:?}"))
}

/// Creates the texture a validated `declaration` describes with its `texels`, through the
/// scene textures' upload plan.
pub(super) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    declaration: &GameTextureDeclaration<'_>,
    texels: &[u8],
) -> Result<wgpu::Texture, GameResourceError> {
    let label = declaration.label;
    let upload_error =
        |error: worldrunner_render_gpu::texture::UploadError| refuse(label, &error.to_string());
    let [width, height, _] = declaration.size;
    let levels = declaration.packed_levels()?;
    let chain: u64 = levels
        .iter()
        .filter(|level| level.layer == 0)
        .map(|level| level.len)
        .sum();
    let chain = usize::try_from(chain).map_err(|_| refuse(label, "the mip chain is too large"))?;
    let chains = texels
        .chunks_exact(chain)
        .map(|layer| {
            packed_mips(
                declaration.format,
                width,
                height,
                declaration.mip_levels,
                layer,
            )
        })
        .collect::<Result<Vec<Vec<MipLevel<'_>>>, _>>()
        .map_err(upload_error)?;
    let layers: Vec<TextureLayer<'_>> = chains
        .iter()
        .map(|mips| TextureLayer {
            format: declaration.format,
            width,
            height,
            mips,
        })
        .collect();
    let kind = match declaration.dimension {
        wgpu::TextureViewDimension::D3 => TextureKind::D3,
        wgpu::TextureViewDimension::D2Array => TextureKind::Array,
        _ => TextureKind::D2,
    };
    TextureUpload::new(&layers, 0, kind)
        .and_then(|plan| plan.upload(device, queue, label, wgpu::TextureUsages::empty()))
        .map_err(upload_error)
}

pub(super) fn refuse(label: &str, reason: &str) -> GameResourceError {
    GameResourceError::Declaration {
        label: label.to_owned(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
#[path = "declared_textures_tests.rs"]
mod tests;
