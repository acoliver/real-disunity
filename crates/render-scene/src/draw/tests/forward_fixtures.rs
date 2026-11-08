//! Card entry variants, mapped decals, alpha-zero coverage and both shadow-normal signs.
use super::*;
use worldrunner_render_plan::{
    SceneImpostor, SceneImpostorChannels, SceneImpostorLayout, SceneTranslucency,
    SceneTranslucencyParts,
};

fn variant(scene: &RenderScene, relief: bool, opaque: bool) -> RenderScene {
    let old = &scene.resources().materials()[0];
    let mut impostor = SceneImpostor::new(
        SceneImpostorLayout::HemisphericalOctahedral,
        [3, 3],
        12_345,
        true,
    )
    .unwrap()
    .with_plane_size([2000, 2000])
    .unwrap()
    .with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion);
    if relief {
        impostor = impostor
            .with_relief(TextureHandle::from_raw(2), 310)
            .unwrap();
    }
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: old.albedo(),
        normal: old.normal(),
        roughness_metallic: old.roughness_metallic(),
        ambient_occlusion: old.ambient_occlusion(),
        self_illumination: old.self_illumination(),
        roughness_micro: old.roughness_micro(),
        metalness_micro: old.metalness_micro(),
        alpha_mode: if opaque {
            SceneAlphaMode::Opaque
        } else {
            old.alpha_mode()
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(impostor);
    replace_material(scene, &material)
}

fn replace_material(scene: &RenderScene, material: &SceneMaterial) -> RenderScene {
    let mut materials = scene.resources().materials().to_vec();
    materials[0] = *material;
    scene
        .with_resources(
            SceneResources::new(scene.resources().textures().to_vec(), materials).unwrap(),
            scene.geometry().clone(),
            scene.effects().cloned(),
        )
        .unwrap()
}

pub(super) fn a2c_scene(scene: &RenderScene) -> RenderScene {
    let mut material = scene.resources().materials()[0];
    if matches!(material.alpha_mode(), SceneAlphaMode::Mask { .. }) {
        material = material.with_alpha_to_coverage(true).unwrap();
    }
    material = material
        .with_translucency(333_337)
        .unwrap()
        .with_translucency_mask(
            SceneTranslucency::new(SceneTranslucencyParts {
                distortion_micro: 217_003,
                falloff_micro: 2_173_003,
                sun_scale_micro: 719_003,
                ambient_scale_micro: 0,
                scale_micro: 691_003,
                base_kelvin: 6500,
                warming_kelvin: 300,
                luminance_micro: 719_003,
                saturation_micro: 0,
            })
            .unwrap(),
        );
    replace_material(scene, &material)
}

fn transparent(scene: &RenderScene, cutoff: u32) -> RenderScene {
    let mut material = scene.resources().materials()[0];
    material = material.with_impostor(
        material
            .impostor()
            .unwrap()
            .with_pass_cutoffs(cutoff, 317_003)
            .unwrap(),
    );
    let scene = replace_material(scene, &material);
    let mut textures = scene.resources().textures().to_vec();
    if let SceneTexture::Texels { bytes, .. } = &mut textures[0] {
        let mut values = bytes.to_vec();
        for pixel in values.chunks_exact_mut(4) {
            pixel[3] = 0;
        }
        *bytes = values.into();
    } else {
        panic!("fixture requires texels");
    }
    scene
        .with_resources(
            SceneResources::new(textures, scene.resources().materials().to_vec()).unwrap(),
            scene.geometry().clone(),
            None,
        )
        .unwrap()
}

fn material_channels(scene: &RenderScene) -> RenderScene {
    let old = &scene.resources().materials()[0];
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: old.albedo(),
        normal: old.normal(),
        roughness_metallic: old.roughness_metallic(),
        ambient_occlusion: old.ambient_occlusion(),
        self_illumination: Some(TextureHandle::from_raw(3)),
        roughness_micro: old.roughness_micro(),
        metalness_micro: old.metalness_micro(),
        alpha_mode: old.alpha_mode(),
        double_sided: true,
    })
    .unwrap()
    .with_impostor(
        old.impostor()
            .unwrap()
            .with_channels(SceneImpostorChannels::Material),
    );
    replace_material(scene, &material)
}

pub(crate) fn scenes() -> Vec<(&'static str, RenderScene, CameraPose)> {
    let mut scenes = alpha_fixtures::scenes();
    let (_, first, pose) = scenes[0].clone();
    for (name, relief, opaque) in [
        ("flat masked", false, false),
        ("flat opaque", false, true),
        ("relief opaque", true, true),
    ] {
        scenes.push((name, variant(&first, relief, opaque), pose));
    }
    scenes.push(("zero alpha with zero cutoff", transparent(&first, 0), pose));
    scenes.push((
        "fully transparent rejected",
        transparent(&first, 317_003),
        pose,
    ));
    scenes.push((
        "separate AO and emission maps",
        material_channels(&first),
        pose,
    ));
    scenes.push(("four sample hard cutoff", first, pose));
    scenes
        .into_iter()
        .map(|(name, scene, pose)| {
            let scene = a2c_scene(&scene);
            let scene = if name == "four sample hard cutoff" {
                let material = scene.resources().materials()[0]
                    .with_alpha_to_coverage(false)
                    .unwrap();
                replace_material(&scene, &material)
            } else {
                scene
            };
            (name, scene, pose)
        })
        .collect()
}
