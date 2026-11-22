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
    // A depth format reports the `Depth` sample type; through a colour slot it is read as a
    // non-filterable float, like a 32-bit float the device cannot filter.
    let cannot_filter = declared == unfilterable
        || (declared == filterable && actual.is_some_and(|actual| actual != filterable));
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
        ..state.unwrap_or(SamplerState::new(Wrap::Clamp, Wrap::Clamp, Filter::Point))
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

/// The function a comparison sampler compares a shader's reference depth with the stored depth by:
/// the reference passes when it is at least the stored depth.
///
/// Offscreen depth is reversed: a camera clears depth to 0 and a pass's `ZTest LEqual` is
/// `GreaterEqual`, so nearer is larger and a fragment at the stored depth or in front of it passes
/// its depth test. The same function makes a comparison read 1 for a receiver at the stored depth
/// or in front of it (see `depth_range`).
pub(super) const SHADOW_COMPARE: wgpu::CompareFunction = wgpu::CompareFunction::GreaterEqual;

/// The state of a comparison sampler: what its name says, else what the texture it is paired with
/// is read with, else linear filtering clamped to the edge.
///
/// An inline sampler name as the shader modules declare it (`sampler_LinearClampCompare`) carries a
/// filter (`Point`, `Linear` or `Trilinear`) and a wrap mode (`Clamp`, `Repeat` or `Mirror`), and a
/// part it leaves out is the texture's. The wrap along w and the anisotropy are always the
/// texture's, since a comparison sampler reads a 2D depth texture and the name has no word for
/// them. A sampler named after its texture, such as `sampler_MainLightShadowmapTexture`, says
/// neither and reads as the texture does.
pub(super) fn comparison_state(name: Option<&str>, paired: Option<&Sampled>) -> SamplerState {
    let texture = paired
        .and_then(|paired| paired.state)
        .unwrap_or(SamplerState::new(
            Wrap::Clamp,
            Wrap::Clamp,
            Filter::Bilinear,
        ));
    let Some(name) = name else { return texture };
    let filter = named(
        name,
        &[
            ("Point", Filter::Point),
            ("Trilinear", Filter::Trilinear),
            ("Linear", Filter::Bilinear),
        ],
    );
    let wrap = named(
        name,
        &[
            ("Clamp", Wrap::Clamp),
            ("Repeat", Wrap::Repeat),
            ("Mirror", Wrap::Mirror),
        ],
    );
    SamplerState {
        wrap_u: wrap.unwrap_or(texture.wrap_u),
        wrap_v: wrap.unwrap_or(texture.wrap_v),
        filter: filter.unwrap_or(texture.filter),
        ..texture
    }
}

/// The value of the first of `words` that `name` contains.
fn named<T: Copy>(name: &str, words: &[(&str, T)]) -> Option<T> {
    words
        .iter()
        .find(|(word, _)| name.contains(word))
        .map(|&(_, value)| value)
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

    fn bilinear(wrap: Wrap) -> SamplerState {
        SamplerState::new(wrap, wrap, Filter::Bilinear)
    }

    fn point(wrap: Wrap) -> SamplerState {
        SamplerState::new(wrap, wrap, Filter::Point)
    }

    #[test]
    fn a_32_bit_float_the_device_cannot_filter_is_read_unfiltered_at_the_nearest_texel() {
        for format in [F::R32Float, F::Rg32Float, F::Rgba32Float] {
            assert_eq!(
                read_unfilterable(
                    FILTERABLE,
                    format,
                    Features::empty(),
                    Some(bilinear(Wrap::Repeat))
                ),
                (UNFILTERABLE, Some(point(Wrap::Repeat))),
                "{format:?}"
            );
            assert_eq!(
                read_unfilterable(FILTERABLE, format, Features::empty(), None),
                (UNFILTERABLE, Some(point(Wrap::Clamp))),
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
                    Some(bilinear(Wrap::Mirror))
                ),
                (FILTERABLE, Some(bilinear(Wrap::Mirror))),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_depth_texture_in_a_colour_slot_is_read_unfiltered_at_the_nearest_texel() {
        for format in [F::Depth16Unorm, F::Depth24Plus, F::Depth32Float] {
            assert_eq!(
                read_unfilterable(
                    FILTERABLE,
                    format,
                    Features::FLOAT32_FILTERABLE,
                    Some(bilinear(Wrap::Clamp))
                ),
                (UNFILTERABLE, Some(point(Wrap::Clamp))),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_filterable_format_keeps_its_filtering_read() {
        for format in [F::Rgba8Unorm, F::R16Float, F::Rgba16Float] {
            assert_eq!(
                read_unfilterable(
                    FILTERABLE,
                    format,
                    Features::empty(),
                    Some(bilinear(Wrap::Clamp))
                ),
                (FILTERABLE, Some(bilinear(Wrap::Clamp))),
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
                Some(bilinear(Wrap::Repeat))
            ),
            (UNFILTERABLE, Some(point(Wrap::Repeat)))
        );
    }

    #[test]
    fn a_sampler_shared_with_a_texture_that_cannot_be_filtered_is_non_filtering() {
        let unfiltered = Sampled {
            state: Some(point(Wrap::Clamp)),
            unfiltered: true,
        };
        let filtered = Sampled {
            state: Some(bilinear(Wrap::Repeat)),
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
        assert_eq!(
            sampler_state(filtering, None, true),
            Some(point(Wrap::Clamp))
        );
    }

    #[test]
    fn a_non_filtering_slot_reads_the_nearest_texel_of_its_state() {
        let non_filtering = wgpu::SamplerBindingType::NonFiltering;
        let paired = Sampled {
            state: Some(bilinear(Wrap::Mirror)),
            unfiltered: false,
        };
        assert_eq!(
            sampler_state(non_filtering, Some(&paired), false),
            Some(point(Wrap::Mirror))
        );
        assert_eq!(
            sampler_state(non_filtering, None, false),
            Some(point(Wrap::Clamp))
        );
    }

    #[test]
    fn a_filtered_pair_keeps_its_own_state_whatever_other_textures_read() {
        let paired = Sampled {
            state: Some(bilinear(Wrap::Repeat)),
            unfiltered: false,
        };
        assert_eq!(
            sampler_state(wgpu::SamplerBindingType::Filtering, Some(&paired), true),
            Some(bilinear(Wrap::Repeat))
        );
    }

    fn comparison(name: Option<&str>, paired: Option<Sampled>) -> SamplerState {
        comparison_state(name, paired.as_ref())
    }

    #[test]
    fn a_comparison_sampler_with_no_name_and_no_texture_state_filters_linearly_clamped() {
        assert_eq!(
            comparison(None, None),
            SamplerState::new(Wrap::Clamp, Wrap::Clamp, Filter::Bilinear)
        );
    }

    #[test]
    fn a_comparison_sampler_named_for_its_texture_reads_as_the_texture_does() {
        let paired = Sampled {
            state: Some(point(Wrap::Repeat)),
            unfiltered: false,
        };
        assert_eq!(
            comparison(Some("sampler_MainLightShadowmapTexture"), Some(paired)),
            point(Wrap::Repeat)
        );
        assert_eq!(comparison(None, Some(paired)), point(Wrap::Repeat));
    }

    #[test]
    fn an_inline_comparison_sampler_name_gives_its_filter_and_wrap() {
        let paired = Sampled {
            state: Some(bilinear(Wrap::Repeat)),
            unfiltered: false,
        };
        assert_eq!(
            comparison(Some("sampler_LinearClampCompare"), Some(paired)),
            bilinear(Wrap::Clamp)
        );
        assert_eq!(
            comparison(Some("sampler_PointClampCompare"), Some(paired)),
            point(Wrap::Clamp)
        );
        assert_eq!(
            comparison(Some("sampler_PointRepeatCompare"), None),
            point(Wrap::Repeat)
        );
        assert_eq!(
            comparison(Some("sampler_TrilinearMirrorCompare"), None),
            SamplerState::new(Wrap::Mirror, Wrap::Mirror, Filter::Trilinear)
        );
    }

    #[test]
    fn a_name_that_gives_only_a_wrap_keeps_the_texture_filter() {
        let paired = Sampled {
            state: Some(point(Wrap::Clamp)),
            unfiltered: false,
        };
        assert_eq!(
            comparison(Some("sampler_RepeatCompare"), Some(paired)),
            point(Wrap::Repeat)
        );
    }

    #[test]
    fn a_comparison_sampler_keeps_the_wrap_along_w_and_the_anisotropy_of_the_texture() {
        let texture = SamplerState {
            wrap_w: Wrap::Mirror,
            anisotropy: 8,
            ..SamplerState::new(Wrap::Repeat, Wrap::Repeat, Filter::Trilinear)
        };
        let paired = Sampled {
            state: Some(texture),
            unfiltered: false,
        };
        let named = comparison(Some("sampler_LinearClampCompare"), Some(paired));
        assert_eq!((named.wrap_w, named.anisotropy), (Wrap::Mirror, 8));
        assert_eq!(
            (named.wrap_u, named.filter),
            (Wrap::Clamp, Filter::Bilinear)
        );
    }

    #[test]
    fn a_stored_depth_passes_a_reference_at_it_or_in_front_of_it() {
        // Depth is reversed: the camera clears to 0 and a pass's LEqual is GreaterEqual.
        assert_eq!(SHADOW_COMPARE, wgpu::CompareFunction::GreaterEqual);
    }
}
