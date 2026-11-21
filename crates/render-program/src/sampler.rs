//! Sampler state a texture input can carry, and the cache that creates one sampler per state.

use std::collections::HashMap;

use worldrunner_render_post::contracts::CheckedSampler;

/// How a texture coordinate outside 0 to 1 is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wrap {
    Repeat,
    Clamp,
    Mirror,
}

/// How texels are filtered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    /// Nearest texel, nearest mip.
    Point,
    /// Linear within a mip, nearest mip.
    Bilinear,
    /// Linear within a mip and between mips.
    Trilinear,
}

/// Wrap along each axis, filter mode and anisotropy of a sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerState {
    pub wrap_u: Wrap,
    pub wrap_v: Wrap,
    pub wrap_w: Wrap,
    pub filter: Filter,
    /// The most texels along the long axis of a sample's footprint that the sampler reads, as
    /// the wgpu `anisotropy_clamp`: 1 reads no more than isotropic filtering. The sampler
    /// applies it only to [`Filter::Trilinear`], since wgpu refuses an anisotropy above 1 unless
    /// the texel filters and the mip filter are all linear; point and bilinear states read with
    /// 1 whatever this holds, and a value outside 1 to 16 is clamped into that range.
    pub anisotropy: u16,
}

/// The most anisotropy wgpu accepts.
const MAX_ANISOTROPY: u16 = 16;

impl Wrap {
    fn address(self) -> wgpu::AddressMode {
        match self {
            Self::Repeat => wgpu::AddressMode::Repeat,
            Self::Clamp => wgpu::AddressMode::ClampToEdge,
            Self::Mirror => wgpu::AddressMode::MirrorRepeat,
        }
    }
}

impl SamplerState {
    /// A state that wraps along w by clamping to the edge and reads without anisotropy: what a
    /// sampler was before it carried the two.
    #[must_use]
    pub const fn new(wrap_u: Wrap, wrap_v: Wrap, filter: Filter) -> Self {
        Self {
            wrap_u,
            wrap_v,
            wrap_w: Wrap::Clamp,
            filter,
            anisotropy: 1,
        }
    }

    fn descriptor(self) -> wgpu::SamplerDescriptor<'static> {
        let (texel, mip) = match self.filter {
            Filter::Point => (wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
            Filter::Bilinear => (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Nearest),
            Filter::Trilinear => (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Linear),
        };
        wgpu::SamplerDescriptor {
            label: Some("render-program sampler"),
            address_mode_u: self.wrap_u.address(),
            address_mode_v: self.wrap_v.address(),
            address_mode_w: self.wrap_w.address(),
            mag_filter: texel,
            min_filter: texel,
            mipmap_filter: mip,
            anisotropy_clamp: match self.filter {
                Filter::Trilinear => self.anisotropy.clamp(1, MAX_ANISOTROPY),
                Filter::Point | Filter::Bilinear => 1,
            },
            ..wgpu::SamplerDescriptor::default()
        }
    }
}

/// The sampler every binding without a state gets: linear filtering, clamped to the edge.
pub(crate) fn fixed_descriptor() -> wgpu::SamplerDescriptor<'static> {
    wgpu::SamplerDescriptor {
        label: Some("render-program fixed sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..wgpu::SamplerDescriptor::default()
    }
}

/// The sampler every non-filtering sampler binding without a state gets: nearest texel and mip,
/// clamped to the edge.
pub(crate) fn non_filtering_descriptor() -> wgpu::SamplerDescriptor<'static> {
    wgpu::SamplerDescriptor {
        label: Some("render-program non-filtering sampler"),
        ..wgpu::SamplerDescriptor::default()
    }
}

/// One checked sampler for the fixed state and one for each state asked for.
pub struct SamplerCache {
    device: wgpu::Device,
    fixed: CheckedSampler,
    by_state: HashMap<SamplerState, CheckedSampler>,
    by_comparison: HashMap<(SamplerState, wgpu::CompareFunction), CheckedSampler>,
}

impl SamplerCache {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            fixed: CheckedSampler::new(device, &fixed_descriptor()),
            by_state: HashMap::new(),
            by_comparison: HashMap::new(),
        }
    }

    /// The sampler for `state`, or the fixed sampler when there is none.
    pub fn get(&mut self, state: Option<SamplerState>) -> CheckedSampler {
        let Some(state) = state else {
            return self.fixed.clone();
        };
        self.by_state
            .entry(state)
            .or_insert_with(|| CheckedSampler::new(&self.device, &state.descriptor()))
            .clone()
    }

    /// The comparison sampler of `state` that returns 1 where `function` holds between a
    /// reference depth and the stored one. It is a sampler of its own, apart from the sampler of
    /// the same `state` that has no comparison, since a shader declares the two as different
    /// kinds of binding.
    pub fn comparison(
        &mut self,
        state: SamplerState,
        function: wgpu::CompareFunction,
    ) -> CheckedSampler {
        self.by_comparison
            .entry((state, function))
            .or_insert_with(|| {
                CheckedSampler::new(
                    &self.device,
                    &wgpu::SamplerDescriptor {
                        label: Some("render-program comparison sampler"),
                        compare: Some(function),
                        ..state.descriptor()
                    },
                )
            })
            .clone()
    }

    /// The comparison samplers made so far.
    #[must_use]
    pub fn comparisons(&self) -> usize {
        self.by_comparison.len()
    }

    /// The states that have a sampler of their own.
    #[must_use]
    pub fn states(&self) -> usize {
        self.by_state.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(filter: Filter, anisotropy: u16) -> SamplerState {
        SamplerState {
            anisotropy,
            ..SamplerState::new(Wrap::Repeat, Wrap::Repeat, filter)
        }
    }

    #[test]
    fn a_trilinear_state_keeps_its_anisotropy() {
        for level in [1, 2, 8, 16] {
            assert_eq!(
                state(Filter::Trilinear, level)
                    .descriptor()
                    .anisotropy_clamp,
                level
            );
        }
    }

    #[test]
    fn a_point_or_bilinear_state_reads_without_anisotropy() {
        for filter in [Filter::Point, Filter::Bilinear] {
            let descriptor = state(filter, 16).descriptor();
            assert_eq!(descriptor.anisotropy_clamp, 1, "{filter:?}");
        }
        let point = state(Filter::Point, 16).descriptor();
        assert_eq!(point.mag_filter, wgpu::FilterMode::Nearest);
        let bilinear = state(Filter::Bilinear, 16).descriptor();
        assert_eq!(bilinear.mipmap_filter, wgpu::MipmapFilterMode::Nearest);
    }

    #[test]
    fn an_anisotropy_outside_one_to_sixteen_is_clamped_into_it() {
        assert_eq!(state(Filter::Trilinear, 0).descriptor().anisotropy_clamp, 1);
        assert_eq!(
            state(Filter::Trilinear, 17).descriptor().anisotropy_clamp,
            16
        );
        let most = state(Filter::Trilinear, u16::MAX).descriptor();
        assert_eq!(most.anisotropy_clamp, 16);
    }

    #[test]
    fn an_anisotropic_descriptor_filters_linearly_everywhere() {
        let descriptor = state(Filter::Trilinear, 16).descriptor();
        assert_eq!(descriptor.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(descriptor.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(descriptor.mipmap_filter, wgpu::MipmapFilterMode::Linear);
    }

    #[test]
    fn each_wrap_axis_maps_to_its_own_address_mode() {
        let descriptor = SamplerState {
            wrap_u: Wrap::Repeat,
            wrap_v: Wrap::Mirror,
            wrap_w: Wrap::Clamp,
            filter: Filter::Bilinear,
            anisotropy: 1,
        }
        .descriptor();
        assert_eq!(descriptor.address_mode_u, wgpu::AddressMode::Repeat);
        assert_eq!(descriptor.address_mode_v, wgpu::AddressMode::MirrorRepeat);
        assert_eq!(descriptor.address_mode_w, wgpu::AddressMode::ClampToEdge);
        let along_w = SamplerState {
            wrap_w: Wrap::Mirror,
            ..state(Filter::Bilinear, 1)
        };
        assert_eq!(
            along_w.descriptor().address_mode_w,
            wgpu::AddressMode::MirrorRepeat
        );
    }

    #[test]
    fn new_clamps_along_w_and_reads_without_anisotropy() {
        let made = SamplerState::new(Wrap::Mirror, Wrap::Repeat, Filter::Trilinear);
        assert_eq!((made.wrap_w, made.anisotropy), (Wrap::Clamp, 1));
    }
}
