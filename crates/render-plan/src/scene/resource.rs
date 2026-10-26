//! Owned renderer-domain textures and materials addressed by dense handles.
//!
//! The tables are closed CPU state: no paths, no dataset identifiers, no
//! borrowed publication data, and no defaults. Every payload length is
//! validated against its exact format arithmetic, so a zero-byte payload
//! survives only where its format permits one.

use crate::scene::error::SceneValidationError;

macro_rules! dense_handle {
    ($(#[$doc:meta])* $name:ident, $table:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Adopts one raw table index.
            #[must_use]
            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }

            /// Returns the raw table index.
            #[must_use]
            pub const fn raw(self) -> u32 {
                self.0
            }
        }
    };
}

dense_handle!(
    /// Dense index of one texture inside a [`SceneResources`] texture table.
    TextureHandle,
    "textures"
);
dense_handle!(
    /// Dense index of one material inside a [`SceneResources`] material table.
    MaterialHandle,
    "materials"
);

/// Closed texel formats with a fixed whole byte length per texel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexelFormat {
    /// One unsigned normalized byte per texel.
    R8Unorm,
    /// Two unsigned normalized bytes per texel.
    Rg8Unorm,
    /// Four unsigned normalized bytes per texel in red, green, blue, alpha order.
    Rgba8Unorm,
    /// Four unsigned normalized bytes per texel in blue, green, red, alpha order.
    Bgra8Unorm,
}

impl TexelFormat {
    /// Returns the exact whole byte length of one texel.
    #[must_use]
    pub const fn bytes_per_texel(self) -> u64 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm => 2,
            Self::Rgba8Unorm | Self::Bgra8Unorm => 4,
        }
    }
}

/// Closed container encodings of one exact encoded texture payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureEncoding {
    /// One `DirectDraw` Surface (`DDS`) container; requires a non-empty payload.
    DdsContainer,
    /// One KTX container; requires a non-empty payload.
    KtxContainer,
    /// Caller-owned exact opaque bytes; a zero-byte payload is a legal empty
    /// payload because the encoding asserts no container header of its own.
    OpaqueExact,
}

impl TextureEncoding {
    /// Returns whether this encoding permits a zero-byte payload.
    #[must_use]
    pub const fn permits_empty_payload(self) -> bool {
        matches!(self, Self::OpaqueExact)
    }
}

/// Closed colour spaces one sampled texture may be authored in.
///
/// Encoded container formats do not reliably carry this distinction, so it is
/// declared alongside the payload rather than inferred from it. Sampling an
/// [`Self::Srgb`] texture as linear, or a [`Self::Linear`] texture as sRGB,
/// silently corrupts every lighting term that consumes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureColorSpace {
    /// Values are sRGB-encoded and must be decoded to linear before lighting.
    Srgb,
    /// Values are already linear and must be sampled unchanged.
    Linear,
}

/// One owned texture as either an explicit texel grid or one exact encoded payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneTexture {
    /// Per-texture sampling with an unchanged owned payload.
    Sampled {
        texture: Box<SceneTexture>,
        sampler: super::TextureSampler,
    },
    /// An explicit texel grid with its own mip chain payload.
    Texels {
        /// Closed texel format of every level.
        format: TexelFormat,
        /// Colour space the stored values are authored in.
        color_space: TextureColorSpace,
        /// Texel width of the base level; strictly positive.
        width: u32,
        /// Texel height of the base level; strictly positive.
        height: u32,
        /// Whole mip levels including the base level; at least one.
        mips: u32,
        /// Concatenated level payloads from the base level upward.
        bytes: Box<[u8]>,
    },
    /// One exact encoded container payload.
    Encoded {
        /// Closed container encoding of the payload.
        encoding: TextureEncoding,
        /// Colour space the decoded values are authored in.
        color_space: TextureColorSpace,
        /// The exact encoded bytes.
        bytes: Box<[u8]>,
    },
}

impl SceneTexture {
    /// Returns the declared colour space of this texture.
    #[must_use]
    pub const fn color_space(&self) -> TextureColorSpace {
        match self {
            Self::Sampled { texture, .. } => texture.color_space(),
            Self::Texels { color_space, .. } | Self::Encoded { color_space, .. } => *color_space,
        }
    }

    /// Validates the payload arithmetic of one owned texture.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when dimensions are zero, the mip
    /// count is zero or unaddressable, the texel byte length does not satisfy
    /// the exact level arithmetic, or a container encoding that requires
    /// content carries an empty payload.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        match self {
            Self::Sampled { texture, .. } => texture.validate(),
            Self::Texels {
                format,
                width,
                height,
                mips,
                bytes,
                ..
            } => validate_texels(*format, *width, *height, *mips, bytes),
            Self::Encoded {
                encoding, bytes, ..
            } => {
                if bytes.is_empty() && !encoding.permits_empty_payload() {
                    return Err(SceneValidationError::EmptyEncodedPayload {
                        part: "texture encoded payload",
                        encoding: encoding_name(*encoding),
                    });
                }
                Ok(())
            }
        }
    }
}

/// Returns the diagnostic name of one container encoding.
const fn encoding_name(encoding: TextureEncoding) -> &'static str {
    match encoding {
        TextureEncoding::DdsContainer => "dds_container",
        TextureEncoding::KtxContainer => "ktx_container",
        TextureEncoding::OpaqueExact => "opaque_exact",
    }
}

/// Validates one explicit texel grid payload.
fn validate_texels(
    format: TexelFormat,
    width: u32,
    height: u32,
    mips: u32,
    bytes: &[u8],
) -> Result<(), SceneValidationError> {
    const PART: &str = "texture texels";
    if width == 0 || height == 0 {
        return Err(SceneValidationError::InvalidDimensions {
            part: PART,
            width,
            height,
        });
    }
    let maximum = max_mips(width, height);
    if mips == 0 || mips > maximum {
        return Err(SceneValidationError::MipOverflow {
            part: PART,
            observed: mips,
            maximum,
        });
    }
    let expected = texel_bytes(format, width, height, mips, PART)?;
    let observed = u64::try_from(bytes.len())
        .map_err(|_| SceneValidationError::ArithmeticOverflow { part: PART })?;
    if expected != observed {
        return Err(SceneValidationError::ByteLengthMismatch {
            part: PART,
            reason: "texel payload must equal the concatenated level sizes",
            expected,
            observed,
        });
    }
    Ok(())
}

/// Highest addressable mip count of one nonzero grid, base level included.
///
/// Levels halve with floor division, so the chain ends once both extents
/// reach one texel: `floor(log2(max(width, height))) + 1` levels exist.
const fn max_mips(width: u32, height: u32) -> u32 {
    let width_bits = u32::BITS - width.leading_zeros();
    let height_bits = u32::BITS - height.leading_zeros();
    if width_bits > height_bits {
        width_bits
    } else {
        height_bits
    }
}

/// Computes the concatenated mip payload size with checked arithmetic.
fn texel_bytes(
    format: TexelFormat,
    width: u32,
    height: u32,
    mips: u32,
    part: &'static str,
) -> Result<u64, SceneValidationError> {
    let bpp = format.bytes_per_texel();
    let mut total = 0_u64;
    for level in 0..mips {
        let divisor = 1_u64 << level;
        let level_width = (u64::from(width) / divisor).max(1);
        let level_height = (u64::from(height) / divisor).max(1);
        let texels = level_width
            .checked_mul(level_height)
            .and_then(|value| value.checked_mul(bpp))
            .ok_or(SceneValidationError::ArithmeticOverflow { part })?;
        total = total
            .checked_add(texels)
            .ok_or(SceneValidationError::ArithmeticOverflow { part })?;
    }
    Ok(total)
}

/// Micro-unit scale shared by explicit material scalar properties.
pub const MATERIAL_MICRO_SCALE: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAlphaMode {
    /// Every texel is fully covered; the alpha channel carries no coverage.
    Opaque,
    /// Texels below `cutoff_micro` alpha are discarded outright.
    Mask {
        /// The exact discard threshold in micro-units of unit alpha.
        cutoff_micro: u32,
    },
}

impl SceneAlphaMode {
    /// Validates the masked cutoff lies in the closed unit interval.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a cutoff exceeds unit alpha.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        match *self {
            Self::Opaque => Ok(()),
            Self::Mask { cutoff_micro } => {
                require_unit_range("material alpha cutoff_micro", u64::from(cutoff_micro))
            }
        }
    }
}

/// The exact material state adopted by [`SceneMaterial::new`].
///
/// The channels are carried as a parts struct because the renderer-domain
/// material addresses five texture channels alongside its scalar and coverage
/// state, which is more positional arguments than a call site can keep honest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneMaterialParts {
    /// The mandatory albedo texture handle.
    pub albedo: TextureHandle,
    /// The optional tangent-space normal map.
    pub normal: Option<TextureHandle>,
    /// The optional packed roughness/metallic map.
    pub roughness_metallic: Option<TextureHandle>,
    /// The optional baked ambient-occlusion map.
    pub ambient_occlusion: Option<TextureHandle>,
    /// The optional self-illumination map.
    pub self_illumination: Option<TextureHandle>,
    /// The roughness scalar in micro-units, multiplying any sampled roughness.
    pub roughness_micro: u32,
    /// The metalness scalar in micro-units, multiplying any sampled metalness.
    pub metalness_micro: u32,
    /// How per-texel coverage is resolved.
    pub alpha_mode: SceneAlphaMode,
    /// Whether back faces are drawn as well as front faces.
    pub double_sided: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneMaterial {
    normal_encoding: super::SceneNormalEncoding,
    normal_y: super::SceneNormalY,
    normal_scale_micro: u32,
    translucency_micro: u32,
    terrain_contact: Option<super::SceneTerrainContact>,
    features: Option<super::SceneMaterialFeatures>,
    albedo: TextureHandle,
    normal: Option<TextureHandle>,
    roughness_metallic: Option<TextureHandle>,
    ambient_occlusion: Option<TextureHandle>,
    self_illumination: Option<TextureHandle>,
    roughness_micro: u32,
    metalness_micro: u32,
    alpha_mode: SceneAlphaMode,
    double_sided: bool,
}

impl SceneMaterial {
    /// Selects normal channels and the source green-axis convention.
    #[must_use]
    pub const fn with_normal_encoding(
        mut self,
        encoding: super::SceneNormalEncoding,
        y: super::SceneNormalY,
    ) -> Self {
        self.normal_encoding = encoding;
        self.normal_y = y;
        self
    }
    pub const fn normal_encoding(&self) -> super::SceneNormalEncoding {
        self.normal_encoding
    }
    pub const fn normal_y(&self) -> super::SceneNormalY {
        self.normal_y
    }
    /// Scales tangent-space XY by a factor between zero and four, in millionths.
    pub fn with_normal_scale(mut self, micro: u32) -> Result<Self, SceneValidationError> {
        super::material_features::bounded("normal scale", micro, 4_000_000)?;
        self.normal_scale_micro = micro;
        Ok(self)
    }
    pub const fn normal_scale_micro(&self) -> u32 {
        self.normal_scale_micro
    }

    /// Reinterprets the metalness channel as a thin-surface mask.
    #[must_use]
    pub fn with_translucency_mask(mut self, value: super::SceneTranslucency) -> Self {
        let features = self.features.get_or_insert(super::SceneMaterialFeatures {
            tint: super::SceneTint::WHITE,
            ambient_occlusion: super::SceneAmbientOcclusion::UNOCCLUDED,
            wind: None,
            layer: None,
            translucency: None,
        });
        features.translucency = Some(value);
        self
    }
    pub fn translucency_mask(&self) -> Option<super::SceneTranslucency> {
        self.features.and_then(|features| features.translucency)
    }

    /// Fraction of backlighting transmitted through a thin surface.
    pub fn with_translucency(mut self, value: u32) -> Result<Self, SceneValidationError> {
        require_unit_range("foliage translucency", u64::from(value))?;
        self.translucency_micro = value;
        Ok(self)
    }
    pub const fn translucency_micro(&self) -> u32 {
        self.translucency_micro
    }
    #[must_use]
    pub const fn with_terrain_contact(mut self, value: super::SceneTerrainContact) -> Self {
        self.terrain_contact = Some(value);
        self
    }
    pub const fn terrain_contact(&self) -> Option<super::SceneTerrainContact> {
        self.terrain_contact
    }

    /// Attaches typed inputs without changing any base material channel.
    pub fn with_features(
        mut self,
        features: super::SceneMaterialFeatures,
    ) -> Result<Self, SceneValidationError> {
        self.features = Some(features);
        Ok(self)
    }
    pub const fn features(&self) -> Option<&super::SceneMaterialFeatures> {
        self.features.as_ref()
    }

    /// Adopts the exact material state after validating scalar ranges.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a scalar property leaves
    /// `0..=1_000_000`.
    pub fn new(parts: SceneMaterialParts) -> Result<Self, SceneValidationError> {
        require_unit_range("material roughness_micro", u64::from(parts.roughness_micro))?;
        require_unit_range("material metalness_micro", u64::from(parts.metalness_micro))?;
        parts.alpha_mode.validate()?;
        Ok(Self {
            normal_encoding: super::SceneNormalEncoding::TwoChannelXy,
            normal_y: super::SceneNormalY::Up,
            normal_scale_micro: 1_000_000,
            translucency_micro: 0,
            terrain_contact: None,
            features: None,
            albedo: parts.albedo,
            normal: parts.normal,
            roughness_metallic: parts.roughness_metallic,
            ambient_occlusion: parts.ambient_occlusion,
            self_illumination: parts.self_illumination,
            roughness_micro: parts.roughness_micro,
            metalness_micro: parts.metalness_micro,
            alpha_mode: parts.alpha_mode,
            double_sided: parts.double_sided,
        })
    }

    /// Returns the mandatory albedo texture handle.
    #[must_use]
    pub const fn albedo(&self) -> TextureHandle {
        self.albedo
    }

    /// Returns the optional normal-map texture handle.
    #[must_use]
    pub const fn normal(&self) -> Option<TextureHandle> {
        self.normal
    }

    /// Returns the optional packed roughness/metallic texture handle.
    #[must_use]
    pub const fn roughness_metallic(&self) -> Option<TextureHandle> {
        self.roughness_metallic
    }

    /// Returns the optional baked ambient-occlusion texture handle.
    #[must_use]
    pub const fn ambient_occlusion(&self) -> Option<TextureHandle> {
        self.ambient_occlusion
    }

    /// Returns the optional self-illumination texture handle.
    #[must_use]
    pub const fn self_illumination(&self) -> Option<TextureHandle> {
        self.self_illumination
    }

    /// Returns the roughness scalar in micro-units.
    #[must_use]
    pub const fn roughness_micro(&self) -> u32 {
        self.roughness_micro
    }

    /// Returns the metalness scalar in micro-units.
    #[must_use]
    pub const fn metalness_micro(&self) -> u32 {
        self.metalness_micro
    }

    /// Returns how per-texel coverage is resolved.
    #[must_use]
    pub const fn alpha_mode(&self) -> SceneAlphaMode {
        self.alpha_mode
    }

    /// Returns whether back faces are drawn as well as front faces.
    #[must_use]
    pub const fn double_sided(&self) -> bool {
        self.double_sided
    }
}

/// Requires one scalar to lie in the closed micro-unit interval.
fn require_unit_range(part: &'static str, observed: u64) -> Result<(), SceneValidationError> {
    if observed > MATERIAL_MICRO_SCALE {
        return Err(SceneValidationError::RangeViolation {
            part,
            minimum: 0,
            maximum: MATERIAL_MICRO_SCALE,
            observed,
        });
    }
    Ok(())
}

/// Closed tables of owned textures and materials addressed by dense handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneResources {
    textures: std::sync::Arc<[SceneTexture]>,
    materials: std::sync::Arc<[SceneMaterial]>,
}

impl SceneResources {
    /// Adopts the tables after validating every texture and material handle.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when one texture payload violates its
    /// format arithmetic or one material references a texture handle outside
    /// the texture table.
    pub fn new(
        textures: Vec<SceneTexture>,
        materials: Vec<SceneMaterial>,
    ) -> Result<Self, SceneValidationError> {
        let resources = Self {
            textures: textures.into(),
            materials: materials.into(),
        };
        resources.validate()?;
        Ok(resources)
    }

    /// Returns the owned texture table.
    #[must_use]
    pub fn textures(&self) -> &[SceneTexture] {
        &self.textures
    }

    /// Returns the owned material table.
    #[must_use]
    pub fn materials(&self) -> &[SceneMaterial] {
        &self.materials
    }

    /// Validates every texture payload and material texture reference.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for texture in self.textures.iter() {
            texture.validate()?;
        }
        let limit = u64::try_from(self.textures.len()).map_err(|_| {
            SceneValidationError::ArithmeticOverflow {
                part: "texture table length",
            }
        })?;
        for material in self.materials.iter() {
            require_texture("material albedo", material.albedo(), limit)?;
            if let Some(normal) = material.normal() {
                require_texture("material normal", normal, limit)?;
            }
            if let Some(roughness_metallic) = material.roughness_metallic() {
                require_texture("material roughness_metallic", roughness_metallic, limit)?;
            }
            if let Some(ambient_occlusion) = material.ambient_occlusion() {
                require_texture("material ambient_occlusion", ambient_occlusion, limit)?;
            }
            if let Some(self_illumination) = material.self_illumination() {
                require_texture("material self_illumination", self_illumination, limit)?;
            }
            if let Some(layer) = material.features().and_then(|features| features.layer) {
                let count = self.materials.len() as u64;
                if u64::from(layer.material().raw()) >= count {
                    return Err(SceneValidationError::HandleOutOfRange {
                        part: "material layer",
                        resource: "material",
                        handle: layer.material().raw(),
                        limit: count,
                    });
                }
                if self.materials[layer.material().raw() as usize]
                    .features()
                    .is_some_and(|features| features.layer.is_some())
                {
                    return Err(SceneValidationError::UnsupportedInput {
                        part: "nested material layers",
                    });
                }
            }
            material.alpha_mode().validate()?;
        }
        Ok(())
    }
}

/// Requires one texture handle to address the texture table.
fn require_texture(
    part: &'static str,
    handle: TextureHandle,
    limit: u64,
) -> Result<(), SceneValidationError> {
    if u64::from(handle.raw()) < limit {
        Ok(())
    } else {
        Err(SceneValidationError::HandleOutOfRange {
            part,
            resource: "texture",
            handle: handle.raw(),
            limit,
        })
    }
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;
