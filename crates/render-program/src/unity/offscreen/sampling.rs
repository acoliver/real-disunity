//! How a draw reads a texture the device cannot filter.
//!
//! A pass declares its textures as filterable floats and samples them through a filtering
//! sampler. A format the device cannot filter (32-bit floats without `FLOAT32_FILTERABLE`, and
//! depth) is bound as a non-filterable float instead, and wgpu refuses a pipeline that pairs one
//! with a filtering sampler. So the sampler that reads such a texture is made non-filtering: it
//! reads the nearest texel and keeps the wrap modes. A device with `FLOAT32_FILTERABLE` reports
//! the format filterable and nothing here changes the draw.

use crate::sampler::{Filter, SamplerState, Wrap};

/// The sampler state a texture is read with, and whether the texture can only be read unfiltered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Sampled {
    pub(super) state: Option<SamplerState>,
    pub(super) unfiltered: bool,
}

/// The texture sample type and sampler state for a texture of `format` that a slot declares as
/// `declared`.
///
/// A texture the slot declares non-filterable, or that is a filterable slot's texture of a format
/// the device cannot filter, is read as a non-filterable float with a nearest sampler. The wrap
/// modes are kept; a texture with no sampler state gets the fixed sampler's clamp to the edge.
pub(super) fn read_unfilterable(
    declared: wgpu::TextureSampleType,
    format: wgpu::TextureFormat,
    features: wgpu::Features,
    state: Option<SamplerState>,
) -> (wgpu::TextureSampleType, Option<SamplerState>) {
    use wgpu::TextureSampleType as Type;
    let filterable = Type::Float { filterable: true };
    let unfilterable = Type::Float { filterable: false };
    let actual = format.sample_type(Some(wgpu::TextureAspect::All), Some(features));
    let cannot_filter =
        declared == unfilterable || (declared == filterable && actual == Some(unfilterable));
    if cannot_filter {
        (unfilterable, Some(nearest(state)))
    } else {
        (declared, state)
    }
}

/// `state` read at the nearest texel.
fn nearest(state: Option<SamplerState>) -> SamplerState {
    SamplerState {
        filter: Filter::Point,
        ..state.unwrap_or(SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Clamp,
            filter: Filter::Point,
        })
    }
}

/// What a sampler shared by several textures reads with: a texture that cannot be filtered
/// decides the sampler, since a filtering one would be refused for it; otherwise the later
/// texture's state stands.
pub(super) fn merge(earlier: Option<&Sampled>, read: Sampled) -> Sampled {
    match earlier {
        Some(earlier) if earlier.unfiltered && !read.unfiltered => *earlier,
        _ => read,
    }
}

/// The state of the sampler a slot of `kind` binds.
///
/// `paired` is what the texture the sampler is paired with reads, if the program names the pair.
/// A sampler no texture parameter pairs with could be read by any of the draw's textures, so it is
/// non-filtering when any of them (`any_unfiltered`) cannot be filtered. A slot the program
/// declares non-filtering always is.
pub(super) fn sampler_state(
    kind: wgpu::SamplerBindingType,
    paired: Option<&Sampled>,
    any_unfiltered: bool,
) -> Option<SamplerState> {
    let state = paired.and_then(|paired| paired.state);
    let unfiltered = paired.map_or(any_unfiltered, |paired| paired.unfiltered);
    if unfiltered || kind == wgpu::SamplerBindingType::NonFiltering {
        Some(nearest(state))
    } else {
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{Features, TextureFormat as F, TextureSampleType as Type};

    const FILTERABLE: Type = Type::Float { filterable: true };
    const UNFILTERABLE: Type = Type::Float { filterable: false };

    fn bilinear(wrap: Wrap) -> Option<SamplerState> {
        Some(SamplerState {
            wrap_u: wrap,
            wrap_v: wrap,
            filter: Filter::Bilinear,
        })
    }

    fn point(wrap: Wrap) -> Option<SamplerState> {
        Some(SamplerState {
            wrap_u: wrap,
            wrap_v: wrap,
            filter: Filter::Point,
        })
    }

    #[test]
    fn a_32_bit_float_the_device_cannot_filter_is_read_unfiltered_at_the_nearest_texel() {
        for format in [F::R32Float, F::Rg32Float, F::Rgba32Float] {
            assert_eq!(
                read_unfilterable(
                    FILTERABLE,
                    format,
                    Features::empty(),
                    bilinear(Wrap::Repeat)
                ),
                (UNFILTERABLE, point(Wrap::Repeat)),
                "{format:?}"
            );
            assert_eq!(
                read_unfilterable(FILTERABLE, format, Features::empty(), None),
                (UNFILTERABLE, point(Wrap::Clamp)),
                "{format:?} without a state"
            );
        }
    }

    #[test]
    fn a_32_bit_float_the_device_can_filter_keeps_its_filtering_read() {
        for format in [F::R32Float, F::Rg32Float, F::Rgba32Float] {
            assert_eq!(
                read_unfilterable(
                    FILTERABLE,
                    format,
                    Features::FLOAT32_FILTERABLE,
                    bilinear(Wrap::Mirror)
                ),
                (FILTERABLE, bilinear(Wrap::Mirror)),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_filterable_format_keeps_its_filtering_read() {
        for format in [F::Rgba8Unorm, F::R16Float, F::Rgba16Float] {
            assert_eq!(
                read_unfilterable(FILTERABLE, format, Features::empty(), bilinear(Wrap::Clamp)),
                (FILTERABLE, bilinear(Wrap::Clamp)),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_slot_that_declares_the_texture_unfilterable_is_read_at_the_nearest_texel_whatever_the_format()
     {
        assert_eq!(
            read_unfilterable(
                UNFILTERABLE,
                F::Rgba8Unorm,
                Features::empty(),
                bilinear(Wrap::Repeat)
            ),
            (UNFILTERABLE, point(Wrap::Repeat))
        );
    }

    #[test]
    fn a_sampler_shared_with_a_texture_that_cannot_be_filtered_is_non_filtering() {
        let unfiltered = Sampled {
            state: point(Wrap::Clamp),
            unfiltered: true,
        };
        let filtered = Sampled {
            state: bilinear(Wrap::Repeat),
            unfiltered: false,
        };
        for (first, second) in [(unfiltered, filtered), (filtered, unfiltered)] {
            let merged = merge(Some(&first), second);
            assert!(merged.unfiltered);
            assert_eq!(merged.state.map(|state| state.filter), Some(Filter::Point));
        }
        assert_eq!(merge(Some(&filtered), filtered), filtered);
        assert_eq!(merge(None, filtered), filtered);
    }

    #[test]
    fn a_sampler_no_texture_is_paired_with_follows_the_draws_textures() {
        let filtering = wgpu::SamplerBindingType::Filtering;
        assert_eq!(sampler_state(filtering, None, false), None);
        assert_eq!(sampler_state(filtering, None, true), point(Wrap::Clamp));
    }

    #[test]
    fn a_non_filtering_slot_reads_the_nearest_texel_of_its_state() {
        let non_filtering = wgpu::SamplerBindingType::NonFiltering;
        let paired = Sampled {
            state: bilinear(Wrap::Mirror),
            unfiltered: false,
        };
        assert_eq!(
            sampler_state(non_filtering, Some(&paired), false),
            point(Wrap::Mirror)
        );
        assert_eq!(
            sampler_state(non_filtering, None, false),
            point(Wrap::Clamp)
        );
    }

    #[test]
    fn a_filtered_pair_keeps_its_own_state_whatever_other_textures_read() {
        let paired = Sampled {
            state: bilinear(Wrap::Repeat),
            unfiltered: false,
        };
        assert_eq!(
            sampler_state(wgpu::SamplerBindingType::Filtering, Some(&paired), true),
            bilinear(Wrap::Repeat)
        );
    }
}
