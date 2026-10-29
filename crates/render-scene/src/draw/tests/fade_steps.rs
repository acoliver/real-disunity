use super::fade_handoff::{assert_partition_masks, handoff_scene, mask};
use super::foliage::geometry_sources;
use super::*;
use worldrunner_render_plan::{SceneFadeBand, SceneFadeDirection, SceneFadeReference};

fn step_scene(
    hashed: bool,
    material_band: bool,
    endpoint: u32,
    directions: &[SceneFadeDirection],
) -> RenderScene {
    let base = handoff_scene(hashed, material_band, directions);
    let mut materials = base.resources().materials().to_vec();
    let mut objects = Vec::new();
    for (index, direction) in directions.iter().enumerate() {
        let band = SceneFadeBand::new(endpoint, endpoint).unwrap();
        let band = if *direction == SceneFadeDirection::Keep {
            band.with_terminal_fade()
        } else {
            band
        };
        if endpoint == 0 {
            materials[index] =
                materials[index].with_fade_reference(SceneFadeReference::new([0, -4000, 1000]));
        }
        let object = SceneObject::new(
            identity(),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(u32::try_from(index).unwrap()),
        );
        if material_band {
            let atlas = materials[index].impostor().unwrap().with_fade(band);
            materials[index] = materials[index].with_impostor(atlas);
            objects.push(object);
        } else {
            objects.push(object.with_fade(band));
        }
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn step_bands_partition_forward_prepass_metadata_and_shadow_at_the_endpoint() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (hashed, material_band) in [(true, false), (false, false), (false, true)] {
        for endpoint in [0, 4000] {
            let lower = step_scene(hashed, material_band, endpoint, &[SceneFadeDirection::Keep]);
            let upper = step_scene(
                hashed,
                material_band,
                endpoint,
                &[SceneFadeDirection::Complement],
            );
            let both = step_scene(
                hashed,
                material_band,
                endpoint,
                &[SceneFadeDirection::Keep, SceneFadeDirection::Complement],
            );
            assert_step_partition(
                &context,
                &layouts,
                [&lower, &upper, &both],
                hashed,
                endpoint,
            );
        }
    }
}

fn assert_step_partition(
    context: &GpuContext,
    layouts: &ScenePipelines,
    scenes: [&RenderScene; 3],
    hashed: bool,
    endpoint: u32,
) {
    for delta in [-1, 0, 1] {
        let boundary = if endpoint == 0 {
            delta == 0
        } else {
            delta <= 0
        };
        for (pass, source) in geometry_sources().iter().enumerate() {
            let pixels =
                scenes.map(|scene| mask(context, layouts, scene, source, pass == 3, 4000 + delta));
            assert_partition_masks(
                &pixels[0],
                &pixels[1],
                &pixels[2],
                hashed,
                f64::from(boundary),
            );
        }
    }
}

#[test]
fn inclusive_step_mirrors_keep_endpoint_ownership_in_forward_and_shadow() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for terminal in [false, true] {
        let base = step_scene(
            true,
            false,
            4000,
            &[if terminal {
                SceneFadeDirection::Keep
            } else {
                SceneFadeDirection::Complement
            }],
        );
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(
                base.resources().textures().to_vec(),
                vec![
                    base.resources().materials()[0]
                        .with_fade_direction(SceneFadeDirection::Inclusive),
                ],
            )
            .unwrap(),
            geometry: base.geometry().clone(),
            terrain: None,
            effects: base.effects().cloned(),
        })
        .unwrap();
        for delta in [-1, 0, 1] {
            for pass in [0, 3] {
                let pixels = mask(
                    &context,
                    &layouts,
                    &scene,
                    &geometry_sources()[pass],
                    pass == 3,
                    4000 + delta,
                );
                assert_step_pixels(&pixels, (delta <= 0) == terminal);
            }
        }
    }
}

fn assert_step_pixels(pixels: &[u8], expected: bool) {
    for y in 30..34 {
        for x in 30..34 {
            let offset = ((y * TARGET + x) * 4) as usize;
            let covered = f32::from_le_bytes(pixels[offset..offset + 4].try_into().unwrap()) < 1.0;
            assert_eq!(covered, expected, "pixel={x},{y}");
        }
    }
}
