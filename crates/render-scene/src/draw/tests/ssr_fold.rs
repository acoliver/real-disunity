use super::*;

fn reflection_material(fold: bool, emissive: bool) -> SceneMaterial {
    use worldrunner_render_plan::{SceneTerrainContact, SceneTint};
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: emissive.then(|| TextureHandle::from_raw(0)),
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    if fold {
        material.with_terrain_contact(
            SceneTerrainContact::new(SceneTint::WHITE, 500_000, 0, 1)
                .unwrap()
                .with_normal_fold(2000, 0)
                .unwrap(),
        )
    } else {
        material
    }
}

fn reflection_terrain(base: &RenderScene) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -3000,
        origin_north_millimeters: -3000,
        sample_spacing_millimeters: 6000,
        heights_millimeters: vec![-4500, 3500, -4500, 3500],
        surface_indices: vec![0; 4],
        surfaces: base.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn fold_reflection_scene(fold: bool) -> RenderScene {
    let base = super::super::fold_translucency::folded_scene(1000);
    let mut vertices = base.geometry().meshes()[0].lods()[0].vertices().to_vec();
    if !fold {
        for vertex in &mut vertices {
            vertex.normal_micro = [-447_214, 0, 894_427];
        }
    }
    let receiver = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let mut effects = effect_parts();
    effects.sun.irradiance_micro = 0;
    effects.ambient.sky_irradiance_micro = 0;
    effects.ambient.ground_irradiance_micro = 0;
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            base.resources().textures().to_vec(),
            vec![
                reflection_material(fold, false),
                base.resources().materials()[1],
                reflection_material(false, true),
            ],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![receiver, card(None)],
            vec![
                base.geometry().objects()[0],
                SceneObject::new(
                    SceneTransform {
                        position_millimeters: [-500, 1200, 500],
                        scale_millionths: 2_000_000,
                        ..identity()
                    },
                    MeshHandle::from_raw(1),
                    MaterialHandle::from_raw(2),
                ),
            ],
        )
        .unwrap(),
        terrain: Some(reflection_terrain(&base)),
        effects: Some(
            SceneEffects::new(effects)
                .unwrap()
                .with_ssr(request())
                .unwrap(),
        ),
    })
    .unwrap()
}

fn reflection_frame(source: &RenderScene) -> FrameUniform {
    let view = SceneView::orthographic(
        CameraPose {
            eye_millimeters: [0, -3000, 2500],
            target_millimeters: [0, 0, 500],
        },
        crate::view::OrthographicLens {
            vertical_half_extent_meters: 1.8,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    FrameUniform::new(&view, source.effects()).unwrap()
}

#[test]
fn intermediate_fold_matches_unit_normal_in_directional_environment_and_ssr() {
    let context = context();
    let maps = environment_faces(&context, true);
    let folded = fold_reflection_scene(true);
    let reference = fold_reflection_scene(false);
    let frame = reflection_frame(&folded);
    let actual = render_with_environment(&context, &folded, 1, &maps, &frame);
    let expected = render_with_environment(&context, &reference, 1, &maps, &frame);
    for component in 0..3 {
        compare_reflections(&context, &actual, &expected, component);
    }
}

fn compare_reflections(
    context: &GpuContext,
    actual: &FrameRenderer,
    expected: &FrameRenderer,
    component: u32,
) {
    let a = actual.ssr.as_ref().unwrap().textures();
    let b = expected.ssr.as_ref().unwrap().textures();
    let factor = channel(context, a[3], 0, component);
    let confidence_a = channel(context, a[4], 0, 3);
    let confidence_b = channel(context, b[4], 0, 3);
    let forward_a = channel(context, a[2], 0, component);
    let forward_b = channel(context, b[2], 0, component);
    let trace_a = channel(context, a[4], 0, component);
    let trace_b = channel(context, b[4], 0, component);
    let final_a = channel(context, actual.targets().scene().texture(), 0, component);
    let final_b = channel(context, expected.targets().scene().texture(), 0, component);
    let mut covered = 0;
    let mut hits = 0;
    for y in 13..21 {
        for x in 12..20 {
            let i = y * 32 + x;
            if factor[i] == 0.0 {
                continue;
            }
            assert!(
                (forward_a[i] - forward_b[i]).abs() < 0.000_2,
                "forward environment differs at {x},{y}: {} {}",
                forward_a[i],
                forward_b[i]
            );
            covered += 1;
            if confidence_a[i] > 0.1 && confidence_b[i] > 0.1 {
                compare_hit(i, &confidence_a, &confidence_b, &trace_a, &trace_b);
                let (lower, upper) = confidence_bounds(&final_b, i, 4);
                assert!(
                    final_a[i] >= lower - 0.002 && final_a[i] <= upper + 0.002,
                    "SSR resolve outside quantized-normal footprint at {x},{y}: {} outside {lower}..{upper}",
                    final_a[i]
                );
                hits += 1;
            }
        }
    }
    assert!(covered > 16);
    assert!(hits > 4, "SSR receiver had no reflected hits: {hits}");
}

fn compare_hit(
    i: usize,
    confidence_a: &[f32],
    confidence_b: &[f32],
    trace_a: &[f32],
    trace_b: &[f32],
) {
    // Compare incident radiance independently of traversal confidence, whose
    // thickness fade is sensitive to f16 normal rounding.
    assert!((trace_a[i] / confidence_a[i] - trace_b[i] / confidence_b[i]).abs() < 0.002);
}
