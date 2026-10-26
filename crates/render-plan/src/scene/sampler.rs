#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureFilter {
    Point,
    #[default]
    Bilinear,
    Trilinear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureWrap {
    #[default]
    Repeat,
    Clamp,
    Mirror,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
