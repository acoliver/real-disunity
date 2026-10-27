//! The optional material block follows the unchanged stock uniform prefix.
use worldrunner_render_plan::{SceneMaterialFeatures, SceneWind};

use super::MATERIAL_UNIFORM_BYTES;

#[expect(
    clippy::cast_precision_loss,
    reason = "quantized scene inputs become GPU floats here"
)]
pub(super) fn encode(
    bytes: &mut [u8; MATERIAL_UNIFORM_BYTES],
    features: Option<&SceneMaterialFeatures>,
) {
    let tint = features.map_or([1_000_000; 3], |value| value.tint.channels());
    let ao = features.map_or(1_000_000, |value| value.ambient_occlusion.factor_micro());
    for (index, value) in tint.into_iter().chain([ao]).enumerate() {
        write_float(bytes, 16 + index * 4, value as f32 / 1_000_000.0);
    }
    if let Some(features) = features {
        if let Some(value) = features.translucency {
            encode_translucency(bytes, value);
        }
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
            encode_foliage(bytes, wind);
            encode_field(bytes, wind);
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
        if let Some([height, minimum]) = contact.normal_fold() {
            write_float(bytes, 104, height as f32 / 1000.0);
            write_float(bytes, 108, minimum as f32 / 1_000_000.0);
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "validated quantized inputs become GPU floats"
)]
fn encode_translucency(bytes: &mut [u8], value: worldrunner_render_plan::SceneTranslucency) {
    let p = value.parts();
    let values = [
        p.distortion_micro,
        p.falloff_micro,
        p.sun_scale_micro,
        p.ambient_scale_micro,
        p.scale_micro,
        p.luminance_micro,
        p.saturation_micro,
    ];
    for (index, value) in values.into_iter().enumerate() {
        write_float(bytes, 112 + index * 4, value as f32 / 1_000_000.0);
    }
    write_float(bytes, 140, p.base_kelvin as f32);
    write_float(bytes, 144, p.warming_kelvin as f32);
    write_float(bytes, 148, 1.0);
}

#[expect(
    clippy::cast_precision_loss,
    reason = "validated quantized inputs become GPU floats"
)]
fn encode_foliage(bytes: &mut [u8], wind: SceneWind) {
    let Some(foliage) = wind.foliage() else {
        return;
    };
    let axis =
        glam::Vec3::from_array(foliage.axis().components_micro().map(|v| v as f32)).normalize();
    for (index, value) in axis.to_array().into_iter().enumerate() {
        write_float(bytes, 160 + index * 4, value);
    }
    write_float(bytes, 172, foliage.curve_micro() as f32 / 1_000_000.0);
    for (index, value) in foliage.origin_millimeters().into_iter().enumerate() {
        write_float(bytes, 224 + index * 4, value as f32 / 1000.0);
    }
    for (index, extent) in foliage.extents_millimeters().into_iter().enumerate() {
        write_float(bytes, 176 + index * 4, extent as f32 / 1000.0);
    }
    write_float(bytes, 184, foliage.strength_micro() as f32 / 1_000_000.0);
    let phase = match foliage.phase() {
        worldrunner_render_plan::SceneWindPhase::WorldPosition => 0.0,
        worldrunner_render_plan::SceneWindPhase::InstancePosition => 1.0,
        worldrunner_render_plan::SceneWindPhase::VertexOcclusion => 2.0,
    };
    write_float(bytes, 188, phase);
    for (index, band) in foliage.bands().into_iter().enumerate() {
        let offset = 192 + index * 16;
        write_float(
            bytes,
            offset + 12,
            band.angular_frequency_micro() as f32 / 1_000_000.0,
        );
        write_float(bytes, offset + 4, band.perturb_micro() as f32 / 1_000_000.0);
        write_float(
            bytes,
            offset + 8,
            band.spatial_phase_micro() as f32 / 1_000_000.0,
        );
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "mesh-local millimetres become scene metres"
)]
pub(crate) fn foliage_origin(wind: SceneWind) -> Option<glam::Vec3> {
    wind.foliage()
        .map(|value| glam::Vec3::from_array(value.origin_millimeters().map(|v| v as f32 / 1000.0)))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "validated atlas units become GPU floats"
)]
pub(super) fn encode_impostor(
    bytes: &mut [u8],
    value: Option<worldrunner_render_plan::SceneImpostor>,
) {
    let Some(value) = value else {
        return;
    };
    if let Some((_, scale)) = value.relief() {
        write_float(bytes, 272, scale as f32 / 1000.0);
    }
    write_float(
        bytes,
        276,
        if value.borders().is_some() { 1.0 } else { 0.0 },
    );
    if let Some(band) = value.fade() {
        for (index, v) in band.millimeters().into_iter().enumerate() {
            write_float(bytes, 280 + index * 4, v as f32 / 1000.0);
        }
    }
    let frames = value.frames();
    let layout = match value.layout() {
        worldrunner_render_plan::SceneImpostorLayout::AzimuthElevation => 1.0,
        worldrunner_render_plan::SceneImpostorLayout::HemisphericalOctahedral => 2.0,
    };
    for (index, number) in [
        frames[0] as f32,
        frames[1] as f32,
        value.border_micro() as f32 / 1_000_000.0,
        layout,
        if value.blend() { 1.0 } else { 0.0 },
        value.direction_distance_millimeters() as f32 / 1000.0,
        value.plane_size_millimeters()[0] as f32 / 1000.0,
        value.plane_size_millimeters()[1] as f32 / 1000.0,
    ]
    .into_iter()
    .enumerate()
    {
        write_float(bytes, 240 + index * 4, number);
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "quantized scene values become GPU floats"
)]
fn encode_field(bytes: &mut [u8], wind: SceneWind) {
    let Some(field) = wind.field() else {
        return;
    };
    for (i, v) in field.origin_millimeters().into_iter().enumerate() {
        write_float(bytes, 288 + i * 4, v as f32 / 1000.0);
    }
    for (i, v) in field.extent_millimeters().into_iter().enumerate() {
        write_float(bytes, 296 + i * 4, v as f32 / 1000.0);
    }
    write_float(bytes, 304, 1.0);
    for (i, modifier) in field.modifiers().into_iter().enumerate() {
        let Some(m) = modifier else {
            continue;
        };
        let offset = 336 + i * 32;
        for (j, v) in m.center_millimeters().into_iter().enumerate() {
            write_float(bytes, offset + j * 4, v as f32 / 1000.0);
        }
        write_float(
            bytes,
            offset + 12,
            m.half_height_millimeters() as f32 / 1000.0,
        );
        for (j, v) in m.radii_millimeters().into_iter().enumerate() {
            write_float(bytes, offset + 16 + j * 4, v as f32 / 1000.0);
        }
        write_float(bytes, offset + 24, m.strength_millimeters() as f32 / 1000.0);
    }
}
