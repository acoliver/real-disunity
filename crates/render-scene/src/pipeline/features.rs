//! The optional material block follows the unchanged stock uniform prefix.
use worldrunner_render_plan::{SceneMaterialFeatures, SceneWind};

use super::MATERIAL_UNIFORM_BYTES;

#[expect(
    clippy::cast_precision_loss,
    reason = "quantized scene inputs become GPU floats here"
)]
pub(super) fn encode(
    bytes: &mut [u8; MATERIAL_UNIFORM_BYTES],
    features: Option<SceneMaterialFeatures>,
) {
    let tint = features.map_or([1_000_000; 3], |value| value.tint.channels());
    let ao = features.map_or(1_000_000, |value| value.ambient_occlusion.factor_micro());
    for (index, value) in tint.into_iter().chain([ao]).enumerate() {
        write_float(bytes, 16 + index * 4, value as f32 / 1_000_000.0);
    }
    if let Some(features) = features {
        bytes[64..68]
            .copy_from_slice(&u32::from(features.ambient_occlusion.vertex()).to_le_bytes());
        if let Some(wind) = features.wind {
            let direction = direction(wind);
            for (index, value) in direction.to_array().into_iter().enumerate() {
                write_float(bytes, 32 + index * 4, value);
            }
            write_float(bytes, 44, wind.amplitude_millimeters() as f32 / 1000.0);
            bytes[48..52].copy_from_slice(&wind.period_milliseconds().to_le_bytes());
            bytes[52..56].copy_from_slice(&wind.phase_micro().to_le_bytes());
        }
    }
}

fn write_float(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[expect(
    clippy::cast_precision_loss,
    reason = "validated unit direction is quantized to millionths"
)]
fn direction(wind: SceneWind) -> glam::Vec3 {
    glam::Vec3::from_array(
        wind.direction()
            .components_micro()
            .map(|value| value as f32),
    )
    .normalize()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "GPU displacement and culling use the same metre conversion"
)]
pub(crate) fn wind_extent(wind: SceneWind) -> glam::Vec3 {
    direction(wind).abs() * (wind.amplitude_millimeters() as f32 / 1000.0)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "validated quantized material values become GPU floats"
)]
pub(super) fn encode_surface(
    bytes: &mut [u8; MATERIAL_UNIFORM_BYTES],
    translucency: u32,
    contact: Option<worldrunner_render_plan::SceneTerrainContact>,
) {
    write_float(bytes, 68, translucency as f32 / 1_000_000.0);
    if let Some(contact) = contact {
        for (index, value) in contact.tint().channels().into_iter().enumerate() {
            write_float(bytes, 80 + index * 4, value as f32 / 1_000_000.0);
        }
        write_float(bytes, 92, contact.distance_millimeters() as f32 / 1000.0);
        write_float(bytes, 96, contact.roughness_micro() as f32 / 1_000_000.0);
        write_float(bytes, 100, contact.metalness_micro() as f32 / 1_000_000.0);
    }
}
