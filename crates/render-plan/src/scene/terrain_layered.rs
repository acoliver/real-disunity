//! Pathless height-aware terrain inputs. Coordinates advance east and north.

use super::{SceneTerrain, SceneTexture, SceneValidationError, TextureColorSpace, TextureHandle};

/// Texture products and mapping for one ordered terrain surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLayer {
    /// First texture slot. Default packing: RGB albedo, linear metallic alpha.
    pub albedo_metallic: TextureHandle,
    /// Second texture slot. Default packing: tangent normal, roughness alpha.
    pub normal_roughness: TextureHandle,
    pub normal_encoding: super::SceneNormalEncoding,
    /// Third texture slot. Default packing: linear red-channel height in [0, 1].
    pub height: TextureHandle,
    /// Positive world-space repeat period before appearance rotation.
    pub repeat_millimeters: u32,
    /// World-space translation applied after appearance rotation.
    pub offset_millimeters: [i32; 2],
    /// Additive roughness and metallic adjustments in millionths.
    pub roughness_metallic_offset_micro: [i32; 2],
}

impl TerrainLayer {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        if self.repeat_millimeters == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "terrain layer repeat",
            });
        }
        if self
            .roughness_metallic_offset_micro
            .iter()
            .any(|x| x.unsigned_abs() > 1_000_000)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "terrain layer material adjustment outside [-1, 1]",
            });
        }
        Ok(())
    }
}

/// Coverage filtering and its corresponding height recurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainSplatFilter {
    /// Bilinear type membership, scaled by 0.75 before height weighting.
    Bilinear,
    /// Fourteen weighted, spatially rotated taps and height-aware crossover.
    Spiral14,
}

/// Colorization channel interpretation, sampled with linear clamp filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainColorizationChannels {
    Rgb,
    Grayscale,
    /// BGR tint and signed alpha roughness adjustment.
    BgrRoughness,
}

/// Low-frequency tint mapping in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainColorization {
    pub texture: TextureHandle,
    pub channels: TerrainColorizationChannels,
    /// World position corresponding to UV zero, in millimeters.
    pub origin_millimeters: [i64; 2],
    /// Positive world extent corresponding to one UV unit.
    pub extent_millimeters: [u32; 2],
}

/// Independent material-cell grid and the terrain's ordered surface palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainLayeredSource {
    /// None preserves the original per-layer packing and declared color space.
    pub channel_map: Option<super::TerrainLayerChannelMap>,
    pub type_grid: TerrainTypeGrid,
    pub filter: TerrainSplatFilter,
    pub colorization: Option<TerrainColorization>,
}

impl TerrainLayeredSource {
    pub(crate) fn validate(&self, terrain: &SceneTerrain) -> Result<(), SceneValidationError> {
        self.type_grid.validate_indices(terrain.surfaces().len())?;
        if terrain.surfaces().len() > 256 {
            return Err(SceneValidationError::UnsupportedInput {
                part: "layered terrain supports at most 256 surfaces",
            });
        }
        for surface in terrain.surfaces() {
            surface
                .appearance()
                .layered
                .ok_or(SceneValidationError::UnsupportedInput {
                    part: "layered terrain requires a layer on every surface",
                })?
                .validate()?;
        }
        if self
            .colorization
            .is_some_and(|map| map.extent_millimeters.contains(&0))
        {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "terrain colorization extent",
            });
        }
        Ok(())
    }

    pub(crate) fn validate_handles(
        &self,
        terrain: &SceneTerrain,
        texture_count: usize,
    ) -> Result<(), SceneValidationError> {
        let limit = texture_count as u64;
        let check = |handle: TextureHandle| {
            if u64::from(handle.raw()) >= limit {
                Err(SceneValidationError::HandleOutOfRange {
                    part: "terrain layered source",
                    resource: "texture",
                    handle: handle.raw(),
                    limit,
                })
            } else {
                Ok(())
            }
        };
        for surface in terrain.surfaces() {
            let layer = surface
                .appearance()
                .layered
                .expect("validated layered source");
            for handle in [layer.albedo_metallic, layer.normal_roughness, layer.height] {
                check(handle)?;
            }
        }
        if let Some(map) = self.colorization {
            check(map.texture)?;
        }
        Ok(())
    }
}

impl TerrainLayeredSource {
    /// Resolves the source map, or the original packing for this layer.
    pub fn channels(
        &self,
        layer: TerrainLayer,
        textures: &[SceneTexture],
    ) -> Result<super::TerrainLayerChannelMap, SceneValidationError> {
        if let Some(map) = self.channel_map {
            return Ok(map);
        }
        let texture = textures.get(layer.albedo_metallic.raw() as usize).ok_or(
            SceneValidationError::HandleOutOfRange {
                part: "terrain layered source",
                resource: "texture",
                handle: layer.albedo_metallic.raw(),
                limit: textures.len() as u64,
            },
        )?;
        Ok(super::TerrainLayerChannelMap::packed(
            layer.normal_encoding,
            texture.color_space(),
        ))
    }

    /// Validates texture references, channel layouts, and data color spaces.
    pub fn validate_resources(
        &self,
        terrain: &SceneTerrain,
        textures: &[SceneTexture],
    ) -> Result<(), SceneValidationError> {
        self.validate(terrain)?;
        self.validate_handles(terrain, textures.len())?;
        for surface in terrain.surfaces() {
            let layer = surface
                .appearance()
                .layered
                .expect("validated layered source");
            for handle in [layer.albedo_metallic, layer.normal_roughness, layer.height] {
                super::terrain_channels::channel_count(&textures[handle.raw() as usize])?;
            }
            self.channels(layer, textures)?.validate(layer, textures)?;
        }
        if let Some(map) = self.colorization {
            validate_colorization(&textures[map.texture.raw() as usize], map.channels)?;
        }
        Ok(())
    }
}

fn validate_colorization(
    texture: &SceneTexture,
    channels: TerrainColorizationChannels,
) -> Result<(), SceneValidationError> {
    texture.validate()?;
    let required = match channels {
        TerrainColorizationChannels::Grayscale => 1,
        TerrainColorizationChannels::Rgb => 3,
        TerrainColorizationChannels::BgrRoughness => 4,
    };
    if super::terrain_channels::channel_count(texture)? < required {
        return Err(SceneValidationError::UnsupportedInput {
            part: "terrain colorization component out of range",
        });
    }
    if channels == TerrainColorizationChannels::Grayscale
        && texture.color_space() != TextureColorSpace::Linear
    {
        return Err(SceneValidationError::UnsupportedInput {
            part: "terrain data textures must be linear",
        });
    }
    Ok(())
}

/// Cell membership independent of geometry resolution. The origin is the lower
/// cell corner; cell centers lie half a spacing east and north of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainTypeGrid {
    dimensions: [u32; 2],
    origin_millimeters: [i64; 2],
    spacing_millimeters: u32,
    indices: Box<[u8]>,
}

impl TerrainTypeGrid {
    pub fn new(
        dimensions: [u32; 2],
        origin_millimeters: [i64; 2],
        spacing_millimeters: u32,
        indices: Vec<u8>,
    ) -> Result<Self, SceneValidationError> {
        if dimensions.contains(&0) || spacing_millimeters == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "terrain type grid",
            });
        }
        let expected = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if indices.len() as u64 != expected {
            return Err(SceneValidationError::TerrainSampleCount {
                part: "terrain type grid",
                expected,
                observed: indices.len() as u64,
            });
        }
        Ok(Self {
            dimensions,
            origin_millimeters,
            spacing_millimeters,
            indices: indices.into_boxed_slice(),
        })
    }
    pub const fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }
    pub const fn origin_millimeters(&self) -> [i64; 2] {
        self.origin_millimeters
    }
    pub const fn spacing_millimeters(&self) -> u32 {
        self.spacing_millimeters
    }
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }

    fn validate_indices(&self, limit: usize) -> Result<(), SceneValidationError> {
        if let Some(index) = self
            .indices
            .iter()
            .find(|index| usize::from(**index) >= limit)
        {
            return Err(SceneValidationError::MaterialIndexOutOfRange {
                part: "terrain type grid",
                index: *index,
                limit: limit as u64,
            });
        }
        Ok(())
    }
}
