//! CPU reference for ordered, height-aware terrain composition.

mod anisotropy;
#[cfg(test)]
mod anisotropy_tests;
mod sampling;
mod splat;
#[cfg(test)]
mod tests;

use super::{composition::MaterialTexel, selection::PageKey};
use glam::Vec3;
use thiserror::Error;
use worldrunner_render_plan::{
    SceneResources, SceneTerrain, TerrainColorizationChannels, TerrainLayeredSource,
    TerrainSplatFilter,
};

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LayeredError {
    #[error("layered terrain source is absent or invalid")]
    Source,
    #[error("reference sampling requires explicit compatible texels")]
    Texture,
    #[error("layered texture {texture} declares {declared} mips, expected one or {expected}")]
    IncompleteMipChain {
        texture: u32,
        declared: u32,
        expected: u32,
    },
    #[error("layer sample is nonfinite, outside its domain, or has an invalid normal")]
    Sample,
    #[error("no layer contributes coverage")]
    Coverage,
}

/// Decoded linear material values before colorization and output quantization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightLayerSample {
    pub albedo: [f32; 3],
    pub normal: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub height: f32,
    pub coverage: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayeredTexel {
    pub albedo: [f32; 3],
    pub normal: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub height: f32,
}

impl LayeredTexel {
    /// RGB albedo with metallic alpha, XYZ normal with roughness alpha, and height.
    pub fn pack(self) -> (MaterialTexel, u8) {
        let color = self.albedo.map(unorm);
        let normal = self.normal.map(super::composition::pack_normal);
        (
            MaterialTexel {
                color: [color[0], color[1], color[2], unorm(self.metallic)],
                normal_roughness: [normal[0], normal[1], normal[2], unorm(self.roughness)],
            },
            unorm(self.height),
        )
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped UNORM quantization"
)]
fn unorm(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Surface order is significant. Normals in the spiral recurrence accumulate
/// without attenuating prior directions; scalar products use the crossover.
pub fn compose_height_layers(
    samples: &[HeightLayerSample],
    filter: TerrainSplatFilter,
    tint: [f32; 3],
    roughness_offset: f32,
) -> Result<LayeredTexel, LayeredError> {
    if samples.is_empty()
        || samples.len() > 256
        || !tint.iter().all(|x| x.is_finite() && *x >= 0.0)
        || !roughness_offset.is_finite()
    {
        return Err(LayeredError::Sample);
    }
    let mut result = LayeredTexel {
        albedo: [0.0; 3],
        normal: [0.0; 3],
        roughness: 0.0,
        metallic: 0.0,
        height: 0.0,
    };
    let mut total = 0.0;
    for sample in samples {
        validate_sample(sample)?;
        let spiral = filter == TerrainSplatFilter::Spiral14;
        let mut strength = sample.coverage * if spiral { 1.0 } else { 0.75 };
        if strength <= 0.0001 {
            continue;
        }
        if !spiral {
            strength *= sample.height.max(0.01);
        }
        let share = (1.0 / (strength + total)) * strength;
        let previous = 1.0 - share + if spiral { result.height } else { 0.0 };
        let current = share + if spiral { sample.height } else { 0.0 };
        let threshold = previous.max(current) - 0.5;
        let current = (current - threshold).max(0.0);
        let previous = (previous - threshold).max(0.0);
        let fraction = current / (current + previous);
        let retain = previous / (current + previous);
        for axis in 0..3 {
            result.albedo[axis] = result.albedo[axis] * retain + sample.albedo[axis] * fraction;
            result.normal[axis] = result.normal[axis] * if spiral { 1.0 } else { retain }
                + sample.normal[axis] * fraction;
        }
        result.roughness = result.roughness * retain + sample.roughness * fraction;
        result.metallic = result.metallic * retain + sample.metallic * fraction;
        total += strength + if spiral { sample.height * share } else { 0.0 };
        if spiral {
            result.height = result.height.max(sample.height * strength);
        }
    }
    if total == 0.0 {
        return Err(LayeredError::Coverage);
    }
    result.normal = Vec3::from_array(result.normal)
        .try_normalize()
        .ok_or(LayeredError::Sample)?
        .to_array();
    for (value, tint) in result.albedo.iter_mut().zip(tint) {
        *value *= tint;
    }
    result.roughness += roughness_offset;
    Ok(result)
}

/// Validated, callback-free CPU source. Uses the source's global type grid,
/// so page gutters sample neighboring content rather than page-local clamps.
pub struct LayeredReference<'a> {
    terrain: &'a SceneTerrain,
    resources: &'a SceneResources,
    source: &'a TerrainLayeredSource,
}

impl<'a> LayeredReference<'a> {
    pub fn new(
        terrain: &'a SceneTerrain,
        resources: &'a SceneResources,
    ) -> Result<Self, LayeredError> {
        terrain.validate().map_err(|_| LayeredError::Source)?;
        let source = terrain.layered_source().ok_or(LayeredError::Source)?;
        source
            .validate_resources(terrain, resources.textures())
            .map_err(|_| LayeredError::Texture)?;
        let reference = Self {
            terrain,
            resources,
            source,
        };
        reference.validate_textures()?;
        Ok(reference)
    }

    pub fn sample(&self, world_meters: [f32; 2]) -> Result<LayeredTexel, LayeredError> {
        self.sample_with_footprint(world_meters, [0.0; 2])
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "reference intentionally uses the shader f32 coordinate and parameter precision"
    )]
    /// Samples an axis-aligned world-space texel footprint in meters.
    pub fn sample_with_footprint(
        &self,
        world_meters: [f32; 2],
        footprint: [f32; 2],
    ) -> Result<LayeredTexel, LayeredError> {
        if !world_meters.iter().chain(&footprint).all(|x| x.is_finite())
            || footprint.iter().any(|x| *x < 0.0)
        {
            return Err(LayeredError::Sample);
        }
        let mut samples = Vec::with_capacity(self.terrain.surfaces().len());
        for (index, surface) in self.terrain.surfaces().iter().enumerate() {
            let appearance = surface.appearance();
            let layer = appearance.layered.ok_or(LayeredError::Source)?;
            let coverage = self.coverage(
                u8::try_from(index).expect("validated palette"),
                world_meters,
                appearance.blur_radius_millimeters,
            );
            let rotation = crate::terrain_uniform::rotation_sin_cos(appearance.rotation_arcminutes);
            let (sine, cosine) = rotation;
            let uv = [
                (world_meters[0] * cosine - world_meters[1] * sine
                    + layer.offset_millimeters[0] as f32 * 0.001)
                    / (layer.repeat_millimeters as f32 * 0.001),
                (world_meters[0] * sine
                    + world_meters[1] * cosine
                    + layer.offset_millimeters[1] as f32 * 0.001)
                    / (layer.repeat_millimeters as f32 * 0.001),
            ];
            let repeat = layer.repeat_millimeters as f32 * 0.001;
            let derivatives = [
                [footprint[0] * cosine / repeat, footprint[0] * sine / repeat],
                [
                    -footprint[1] * sine / repeat,
                    footprint[1] * cosine / repeat,
                ],
            ];
            samples.push(self.material_sample(&appearance, uv, derivatives, coverage, rotation)?);
        }
        let mut tint = [1.0; 3];
        let mut roughness = 0.0;
        if let Some(map) = self.source.colorization {
            let uv = std::array::from_fn(|axis| {
                (world_meters[axis] - map.origin_millimeters[axis] as f32 * 0.001)
                    / (map.extent_millimeters[axis] as f32 * 0.001)
            });
            let color = self.texture(map.texture, uv, false)?;
            (tint, roughness) = colorization_values(map.channels, color);
        }
        compose_height_layers(&samples, self.source.filter, tint, roughness)
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "global f64 page centers convert to the same f32 world positions as screen sampling"
    )]
    pub fn compose_page(
        &self,
        settings: worldrunner_render_plan::TerrainPageSettings,
        key: PageKey,
    ) -> Result<Vec<LayeredTexel>, LayeredError> {
        let extent = settings.extent();
        let mut output = Vec::new();
        let footprint = [self.terrain.columns() - 1, self.terrain.rows() - 1].map(|samples| {
            f64::from(samples) * f64::from(self.terrain.sample_spacing_millimeters()) * 0.001
        });
        let footprint = std::array::from_fn(|axis| {
            (footprint[axis]
                / f64::from(settings.parts().interior[axis])
                / f64::from(1_u32 << key.depth())) as f32
        });
        for y in 0..extent[1] {
            for x in 0..extent[0] {
                let uv = super::composition::source_uv(settings, key, [x, y]);
                let world = [
                    self.terrain.origin_east_millimeters() as f64 * 0.001
                        + uv[0]
                            * f64::from(self.terrain.columns() - 1)
                            * f64::from(self.terrain.sample_spacing_millimeters())
                            * 0.001,
                    self.terrain.origin_north_millimeters() as f64 * 0.001
                        + uv[1]
                            * f64::from(self.terrain.rows() - 1)
                            * f64::from(self.terrain.sample_spacing_millimeters())
                            * 0.001,
                ];
                output.push(self.sample_with_footprint(world.map(|x| x as f32), footprint)?);
            }
        }
        Ok(output)
    }
}

fn validate_sample(sample: &HeightLayerSample) -> Result<(), LayeredError> {
    if !sample.albedo.iter().all(|x| x.is_finite() && *x >= 0.0)
        || !sample.normal.iter().all(|x| x.is_finite())
        || Vec3::from_array(sample.normal).length_squared() <= f32::MIN_POSITIVE
        || ![sample.roughness, sample.metallic]
            .iter()
            .all(|x| x.is_finite())
        || ![sample.height, sample.coverage]
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.000_001).contains(x))
    {
        return Err(LayeredError::Sample);
    }
    Ok(())
}

fn decode_normal(
    sample: [f32; 4],
    encoding: worldrunner_render_plan::SceneNormalEncoding,
) -> [f32; 3] {
    let mut direction = std::array::from_fn(|axis| sample[axis] * 2.0 - 1.0);
    if encoding == worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy {
        direction[2] = (1.0 - direction[0] * direction[0] - direction[1] * direction[1])
            .max(0.0)
            .sqrt();
    }
    direction
}

fn colorization_values(channels: TerrainColorizationChannels, color: [f32; 4]) -> ([f32; 3], f32) {
    let (tint, roughness) = match channels {
        TerrainColorizationChannels::Rgb => ([color[0], color[1], color[2]], 0.0),
        TerrainColorizationChannels::Grayscale => ([color[0]; 3], 0.0),
        TerrainColorizationChannels::BgrRoughness => {
            ([color[2], color[1], color[0]], (color[3] - 0.5) * 2.0)
        }
    };
    (tint.map(|value| value * (255.0 / 200.0)), roughness)
}
