//! A render texture of the host, offered to the UI overlay to sample.
use worldrunner_render_plan::TextureSampler;
use worldrunner_world_simulation::graphics::RenderTextureId;

/// Why the overlay cannot sample a texture. [`reason`](Self::reason) is the name a report counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTargetRefusal {
    NotTwoDimensional,
    Multisampled,
    NotSampled,
    DepthOrStencil,
    NotFilterable,
}

impl UiTargetRefusal {
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NotTwoDimensional => "not-2d",
            Self::Multisampled => "multisampled",
            Self::NotSampled => "not-sampled",
            Self::DepthOrStencil => "depth-or-stencil",
            Self::NotFilterable => "not-filterable",
        }
    }
}

/// The one rule for what a UI quad can sample: a single-sample 2D colour texture with texture
/// binding whose format filters as float.
pub(crate) fn refusal(
    dimension: wgpu::TextureDimension,
    samples: u32,
    usage: wgpu::TextureUsages,
    format: wgpu::TextureFormat,
) -> Option<UiTargetRefusal> {
    if dimension != wgpu::TextureDimension::D2 {
        return Some(UiTargetRefusal::NotTwoDimensional);
    }
    if samples > 1 {
        return Some(UiTargetRefusal::Multisampled);
    }
    if !usage.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        return Some(UiTargetRefusal::NotSampled);
    }
    if format.is_depth_stencil_format() {
        return Some(UiTargetRefusal::DepthOrStencil);
    }
    let filters = matches!(
        format.sample_type(Some(wgpu::TextureAspect::All), None),
        Some(wgpu::TextureSampleType::Float { filterable: true })
    );
    (!filters).then_some(UiTargetRefusal::NotFilterable)
}

/// A render texture the UI reads: the world's id, the host's GPU texture, and the sampler the
/// world describes. A texture of `None` has no storage yet and shows as transparent black.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiTarget {
    id: RenderTextureId,
    texture: Option<wgpu::Texture>,
    sampler: TextureSampler,
}

impl UiTarget {
    /// # Errors
    /// Names what the overlay cannot sample about `texture`.
    pub fn new(
        id: RenderTextureId,
        texture: Option<wgpu::Texture>,
        sampler: TextureSampler,
    ) -> Result<Self, UiTargetRefusal> {
        if let Some(held) = &texture {
            let usage = held.usage();
            let format = held.format();
            if let Some(refused) = refusal(held.dimension(), held.sample_count(), usage, format) {
                return Err(refused);
            }
        }
        Ok(Self {
            id,
            texture,
            sampler,
        })
    }

    #[must_use]
    pub const fn id(&self) -> RenderTextureId {
        self.id
    }

    #[must_use]
    pub const fn texture(&self) -> Option<&wgpu::Texture> {
        self.texture.as_ref()
    }

    #[must_use]
    pub const fn sampler(&self) -> TextureSampler {
        self.sampler
    }
}

#[cfg(test)]
mod tests {
    use super::{UiTargetRefusal, refusal};
    use wgpu::{TextureDimension as Dim, TextureFormat as Format, TextureUsages as Usage};

    const SAMPLED: Usage = Usage::TEXTURE_BINDING.union(Usage::RENDER_ATTACHMENT);

    #[test]
    fn a_single_sample_2d_colour_texture_is_sampled() {
        for format in [
            Format::Rgba8Unorm,
            Format::Rgba8UnormSrgb,
            Format::Rgba16Float,
        ] {
            assert_eq!(refusal(Dim::D2, 1, SAMPLED, format), None);
        }
    }

    #[test]
    fn a_multisampled_texture_is_refused_by_name() {
        let refused = refusal(Dim::D2, 4, SAMPLED, Format::Rgba8Unorm);
        assert_eq!(refused, Some(UiTargetRefusal::Multisampled));
        assert_eq!(refused.map(UiTargetRefusal::reason), Some("multisampled"));
    }

    #[test]
    fn a_volume_a_depth_format_and_an_unsampled_texture_are_refused_by_name() {
        let cases = [
            (
                Dim::D3,
                SAMPLED,
                Format::Rgba8Unorm,
                UiTargetRefusal::NotTwoDimensional,
            ),
            (
                Dim::D2,
                Usage::RENDER_ATTACHMENT,
                Format::Rgba8Unorm,
                UiTargetRefusal::NotSampled,
            ),
            (
                Dim::D2,
                SAMPLED,
                Format::Depth32Float,
                UiTargetRefusal::DepthOrStencil,
            ),
            (
                Dim::D2,
                SAMPLED,
                Format::Depth24PlusStencil8,
                UiTargetRefusal::DepthOrStencil,
            ),
        ];
        for (dimension, usage, format, expected) in cases {
            assert_eq!(
                refusal(dimension, 1, usage, format),
                Some(expected),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_format_that_does_not_filter_as_float_is_refused() {
        for format in [Format::Rgba32Float, Format::R32Uint] {
            assert_eq!(
                refusal(Dim::D2, 1, SAMPLED, format),
                Some(UiTargetRefusal::NotFilterable),
                "{format:?}"
            );
        }
    }
}
