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

/// Wrap and filter mode of a sampler. Anisotropy is not carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerState {
    pub wrap_u: Wrap,
    pub wrap_v: Wrap,
    pub filter: Filter,
}

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
            mag_filter: texel,
            min_filter: texel,
            mipmap_filter: mip,
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

/// One checked sampler for the fixed state and one for each state asked for.
pub struct SamplerCache {
    device: wgpu::Device,
    fixed: CheckedSampler,
    by_state: HashMap<SamplerState, CheckedSampler>,
}

impl SamplerCache {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            fixed: CheckedSampler::new(device, &fixed_descriptor()),
            by_state: HashMap::new(),
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

    /// The states that have a sampler of their own.
    #[must_use]
    pub fn states(&self) -> usize {
        self.by_state.len()
    }
}
