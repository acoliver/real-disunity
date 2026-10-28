use std::collections::BTreeSet;
use worldrunner_render_plan::{RenderScene, SceneFadeCoverage, TextureHandle, threshold_texels};

fn handles(scene: &RenderScene) -> BTreeSet<TextureHandle> {
    scene
        .resources()
        .materials()
        .iter()
        .filter_map(|material| match material.fade_coverage() {
            SceneFadeCoverage::Hashed => None,
            SceneFadeCoverage::Threshold { texture, .. } => Some(texture),
        })
        .collect()
}

// Point/repeat loads need no sampler or extra sampled-texture slot. Store the
// caller's R8 grid after the height samples in the existing read-only binding.
fn references(scene: &RenderScene) -> BTreeSet<worldrunner_render_plan::SceneFadeReference> {
    scene
        .resources()
        .materials()
        .iter()
        .filter_map(worldrunner_render_plan::SceneMaterial::fade_reference)
        .collect()
}

pub(super) fn append(scene: &RenderScene, bytes: &mut Vec<u8>) {
    for handle in handles(scene) {
        let (size, texels) = threshold_texels(&scene.resources().textures()[handle.raw() as usize])
            .expect("threshold admission precedes preparation");
        bytes.extend_from_slice(&size.to_le_bytes());
        for value in texels {
            bytes.extend_from_slice(&(f32::from(*value) / 255.0).to_le_bytes());
        }
    }
    for reference in references(scene) {
        for value in reference.offset_millimeters() {
            bytes.extend_from_slice(
                &crate::geometry::millimeter_to_meter(i64::from(value)).to_le_bytes(),
            );
        }
        let clamp = reference
            .eye_vertical_clamp_millimeters()
            .map_or(-1.0, |v| crate::geometry::millimeter_to_meter(i64::from(v)));
        bytes.extend_from_slice(&clamp.to_le_bytes());
    }
}

pub(super) fn uniform(
    scene: &RenderScene,
    material: &worldrunner_render_plan::SceneMaterial,
) -> Vec<u8> {
    let mut bytes = crate::pipeline::MaterialUniform::from_material(material)
        .to_bytes()
        .to_vec();
    if let SceneFadeCoverage::Threshold { texture, .. } = material.fade_coverage() {
        let offset: usize = references(scene).len() * 4
            + handles(scene)
                .range(texture..)
                .map(|handle| {
                    let (_, data) =
                        threshold_texels(&scene.resources().textures()[handle.raw() as usize])
                            .expect("validated threshold");
                    data.len() + 1
                })
                .sum::<usize>();
        bytes[436..440].copy_from_slice(
            &u32::try_from(offset)
                .expect("admitted storage size")
                .to_le_bytes(),
        );
    }
    if let Some(reference) = material.fade_reference() {
        let offset = u32::try_from(references(scene).range(reference..).count() * 4)
            .expect("admitted reference storage size");
        let flags = u32::from_le_bytes(bytes[432..436].try_into().unwrap());
        let packed = offset
            .checked_shl(8)
            .filter(|v| *v >> 8 == offset)
            .expect("fade reference storage exceeds policy address range");
        bytes[432..436].copy_from_slice(&(flags | packed).to_le_bytes());
    }
    bytes
}
