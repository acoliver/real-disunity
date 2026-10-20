use crate::ReceiverDimension;

/// Coverage topology, independent of how a host fits its light matrices or updates ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionalCoverage {
    Single,
    Cascades { count: u32 },
}

impl DirectionalCoverage {
    pub const fn layers(self) -> u32 {
        match self {
            Self::Single => 1,
            Self::Cascades { count } => count,
        }
    }
}

/// Explicit filter footprint. Derivatives for receiver-plane correction must be evaluated
/// unconditionally by the fragment caller before any coverage or cascade branch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowFilter {
    Center,
    FourTap { half_extent_texels: f32 },
    Grid { radius: u32, receiver_plane: bool },
}

/// No game defaults: hosts select topology, comparison rules and both bias stages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalShadowConfig {
    pub coverage: DirectionalCoverage,
    pub dimension: ReceiverDimension,
    pub resolution: u32,
    pub caster_compare: wgpu::CompareFunction,
    pub receiver_compare: wgpu::CompareFunction,
    pub caster_bias: wgpu::DepthBiasState,
    pub receiver_depth_bias: f32,
    pub receiver_normal_bias: f32,
    pub filter: ShadowFilter,
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid directional shadow topology, resolution, bias or filter")]
pub struct InvalidShadowConfig;

impl DirectionalShadowConfig {
    pub fn validate(self, limits: &wgpu::Limits) -> Result<Self, InvalidShadowConfig> {
        let layers = self.coverage.layers();
        let valid_filter = match self.filter {
            ShadowFilter::Center => true,
            ShadowFilter::FourTap { half_extent_texels } => {
                half_extent_texels.is_finite() && half_extent_texels >= 0.0
            }
            ShadowFilter::Grid { radius, .. } => (1..=4).contains(&radius),
        };
        if layers == 0
            || layers > limits.max_texture_array_layers
            || (self.dimension == ReceiverDimension::D2 && layers != 1)
            || self.resolution == 0
            || self.resolution > limits.max_texture_dimension_2d
            || !self.caster_bias.slope_scale.is_finite()
            || !self.caster_bias.clamp.is_finite()
            || !self.receiver_depth_bias.is_finite()
            || !(0.0..1.0).contains(&self.receiver_depth_bias)
            || !self.receiver_normal_bias.is_finite()
            || self.receiver_normal_bias < 0.0
            || !valid_filter
        {
            return Err(InvalidShadowConfig);
        }
        Ok(self)
    }

    pub fn comparison_sampler(self, device: &wgpu::Device) -> wgpu::Sampler {
        crate::comparison_sampler(device, self.receiver_compare)
    }
}
