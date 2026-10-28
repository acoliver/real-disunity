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
pub(super) fn append(scene: &RenderScene, bytes: &mut Vec<u8>) {
    for handle in handles(scene) {
        let (size, texels) = threshold_texels(&scene.resources().textures()[handle.raw() as usize])
            .expect("threshold admission precedes preparation");
        bytes.extend_from_slice(&size.to_le_bytes());
        for value in texels {
            bytes.extend_from_slice(&(f32::from(*value) / 255.0).to_le_bytes());
        }
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
        let offset: usize = handles(scene)
            .range(texture..)
            .map(|handle| {
                let (_, data) =
                    threshold_texels(&scene.resources().textures()[handle.raw() as usize])
                        .expect("validated threshold");
                data.len() + 1
            })
            .sum();
        bytes[436..440].copy_from_slice(
            &u32::try_from(offset)
                .expect("admitted storage size")
                .to_le_bytes(),
        );
    }
    bytes
}
