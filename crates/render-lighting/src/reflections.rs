//! Linear-HDR reflection composition, independent of exposure and display encoding.
//! Radiance mips contain incident light, before the split-sum BRDF. The forward
//! environment lobe must use the same F0, LUT coefficients and radiometric scale.
use glam::Vec3;

/// Callers select composition explicitly; neither algorithm is inferred from inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Composition {
    /// Adds a lobe when the forward shader has no environment specular.
    Additive,
    /// Blends against the already shaded environment-specular lobe only.
    ReplaceEnvironment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    None,
    Opaque,
    Water,
}

/// Evaluate independently for each forward sample. Water metadata is written
/// only by depth-tested water fragments; opaque metadata must match final depth.
pub fn select_receiver(water_covered: bool, final_depth: f32, opaque_depth: f32) -> Receiver {
    if water_covered {
        Receiver::Water
    } else if (final_depth - opaque_depth).abs() <= (opaque_depth * 0.0001).max(0.001) {
        Receiver::Opaque
    } else {
        Receiver::None
    }
}

/// R7's BRDF LUT supplies A and B; neither term includes exposure or radiance.
pub fn split_sum_factor(f0: Vec3, coefficients: [f32; 2]) -> Vec3 {
    f0 * coefficients[0] + Vec3::splat(coefficients[1])
}

/// Delta to add to forward color. `premultiplied_radiance` is filtered RGB times
/// confidence, as produced by the SSR trace. `environment_specular` is the actual
/// forward lobe after BRDF, fog transmittance and exposure, in the same linear HDR
/// space as incident radiance. `factor` includes the BRDF and camera
/// transmittance, not exposure already applied to incident radiance. Stock HDR
/// geometry is unexposed because post owns exposure.
pub fn composition_delta(
    algorithm: Composition,
    premultiplied_radiance: Vec3,
    environment_specular: Vec3,
    factor: Vec3,
    confidence: f32,
) -> Vec3 {
    assert!((0.0..=1.0).contains(&confidence));
    if confidence == 0.0 {
        return Vec3::ZERO;
    }
    let replacement = premultiplied_radiance * factor;
    match algorithm {
        Composition::Additive => replacement,
        Composition::ReplaceEnvironment => replacement - environment_specular * confidence,
    }
}

/// Adjacent mip levels and interpolation weight for perceptual roughness.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite normalized roughness and bounded texture mip count"
)]
pub fn radiance_lod(roughness: f32, levels: u32) -> (u32, u32, f32) {
    assert!((0.0..=1.0).contains(&roughness) && (1..=32).contains(&levels));
    let lod = roughness * (levels - 1) as f32;
    let lower = lod.floor() as u32;
    (lower, (lower + 1).min(levels - 1), lod.fract())
}
