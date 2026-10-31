#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureFilter {
    Point,
    #[default]
    Bilinear,
    Trilinear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureWrap {
    #[default]
    Repeat,
    Clamp,
    Mirror,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextureSampler {
    pub filter: TextureFilter,
    pub wrap_u: TextureWrap,
    pub wrap_v: TextureWrap,
}
impl super::SceneTexture {
    #[must_use]
    pub fn with_sampler(self, sampler: TextureSampler) -> Self {
        let texture = match self {
            Self::Sampled { texture, .. } => *texture,
            other => other,
        };
        Self::Sampled {
            texture: Box::new(texture),
            sampler,
        }
    }
    pub fn sampler(&self) -> TextureSampler {
        match self {
            Self::Sampled { sampler, .. } => *sampler,
            _ => TextureSampler {
                filter: TextureFilter::Trilinear,
                ..Default::default()
            },
        }
    }
    pub fn payload(&self) -> &Self {
        match self {
            Self::Sampled { texture, .. } => texture.payload(),
            other => other,
        }
    }
}

/// Validated maximum anisotropic footprint ratio. One preserves isotropic sampling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureAnisotropy(u16);

impl TextureAnisotropy {
    pub const ISOTROPIC: Self = Self(1);

    /// Accepts only 1, 2, 4, 8 or 16.
    pub fn new(level: u16) -> Result<Self, super::SceneValidationError> {
        if !matches!(level, 1 | 2 | 4 | 8 | 16) {
            return Err(super::SceneValidationError::UnsupportedInput {
                part: "texture anisotropy must be 1, 2, 4, 8 or 16",
            });
        }
        Ok(Self(level))
    }

    pub const fn level(self) -> u16 {
        self.0
    }
}

impl Default for TextureAnisotropy {
    fn default() -> Self {
        Self::ISOTROPIC
    }
}
