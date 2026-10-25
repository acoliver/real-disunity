use super::*;

pub(super) fn source_with(source: &RenderScene, shadows: SceneShadowCascades) -> RenderScene {
    let mut effects = fixture::effects_parts(600_000);
    effects.shadows = shadows;
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: source.terrain().cloned(),
        effects: Some(SceneEffects::new(effects).unwrap()),
    })
    .unwrap()
}

#[test]
fn gpu_four_layers_have_distinct_coverage_and_keep_retained_handles() {
    let context = context();
    let source = source_with(
        &scene(
            &[
                [0, 0, 1_000],
                [0, 1_100, 1_000],
                [0, 1_700, 1_000],
                [0, 2_600, 1_000],
            ],
            600_000,
            None,
        ),
        SceneShadowCascades::new(vec![650, 1_200, 2_000, 3_500], 100_000).unwrap(),
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    assert_eq!(renderer.map.depth.layers(), 4);
    let handle = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let image = target.read_rgba8(&context).unwrap();
    for y in [0.0, 1.1, 1.7, 2.6] {
        assert_shadow(&image, &baseline, Vec3::new(-0.75, y, 0.0));
    }
    let drawn: Vec<_> = (0..4)
        .map(|i| {
            renderer
                .cascade_view_stats(i)
                .drawn_per_lod
                .iter()
                .sum::<usize>()
        })
        .collect();
    assert!(drawn.windows(2).all(|w| w[0] < w[1]), "{drawn:?}");
    renderer
        .scene_mut()
        .set_instance_visible(handle, false)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let hidden = target.read_rgba8(&context).unwrap();
    assert_lit(&hidden, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    renderer
        .scene_mut()
        .set_instance_visible(handle, true)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), image);
}

#[test]
fn gpu_each_cascade_keeps_off_camera_casters_and_main_camera_lod() {
    let context = context();
    let base = source_with(
        &scene(&[[1_500, 0, 2_000]], 600_000, None),
        SceneShadowCascades::new(vec![1_000, 2_000, 3_000, 4_000], 100_000).unwrap(),
    );
    let source = with_caster_lod(&base);
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4_000, 1_500],
            target_millimeters: [0, 0, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 30_000,
            ..*source.effects().unwrap().camera()
        },
        view().viewport(),
    )
    .unwrap();
    let mut renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            depth_half_extent_meters: 0.5,
            ..settings()
        },
    )
    .unwrap();
    let handle = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    let image = target.read_rgba8(&context).unwrap();
    assert_eq!(renderer.view_stats().culled, 1);
    for index in 0..4 {
        assert_eq!(renderer.cascade_view_stats(index).drawn_per_lod, vec![1, 1]);
    }
    renderer
        .scene_mut()
        .set_instance_visible(handle, false)
        .unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    let plain = target.read_rgba8(&context).unwrap();
    assert!(
        i16::from(at_view(&plain, Vec3::ZERO, &view))
            - i16::from(at_view(&image, Vec3::ZERO, &view))
            > 40
    );
    for index in 0..4 {
        assert_eq!(renderer.cascade_view_stats(index).drawn_per_lod, vec![1, 0]);
    }
}

#[test]
fn gpu_camera_splits_blend_and_reconfigure_without_reupload() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, Some([255, 255, 0, 0]));
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let handle = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for camera_splits in [true, false] {
        for count in [2_u32, 3, 4, 2] {
            let count_usize = usize::try_from(count).unwrap();
            let policy = if camera_splits {
                SceneShadowCascades::camera_splits(
                    [4_000, 8_000, 12_000, 20_000][..count_usize].to_vec(),
                    200_000,
                )
            } else {
                SceneShadowCascades::new(
                    [1_000, 2_000, 3_000, 4_000][..count_usize].to_vec(),
                    200_000,
                )
            }
            .unwrap();
            let configured = source_with(&source, policy);
            renderer.set_effects(configured.effects().unwrap()).unwrap();
            render_target(&mut renderer, &context, &view(), &target).unwrap();
            assert_eq!(renderer.map.depth.layers(), count);
            assert!(renderer.scene_mut().instance(handle).is_ok());
            let image = target.read_rgba8(&context).unwrap();
            let baseline = capture_scene(&context, &configured, &view()).unwrap();
            assert_shadow(&image, &baseline, Vec3::new(-0.93, 0.0, 0.0));
            assert_lit(&image, &baseline, Vec3::new(-0.57, 0.0, 0.0));
            for index in 0..count_usize {
                assert!(
                    renderer
                        .cascade_view_stats(index)
                        .drawn_per_lod
                        .iter()
                        .sum::<usize>()
                        > 0
                );
            }
        }
    }
    let bad = source_with(
        &source,
        SceneShadowCascades::camera_splits(vec![10, 20, 30, 40], 0).unwrap(),
    );
    assert!(renderer.set_effects(bad.effects().unwrap()).is_err());
}

fn with_caster_lod(base: &RenderScene) -> RenderScene {
    let lod = &base.geometry().meshes()[1].lods()[0];
    let second =
        SceneMeshLod::new(1, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(
            vec![
                base.geometry().meshes()[0].clone(),
                SceneMesh::with_distances(vec![lod.clone(), second], vec![1_000, 20_000]).unwrap(),
            ],
            base.geometry().objects().to_vec(),
        )
        .unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn gpu_four_cascade_terrain_receivers_share_masked_coverage() {
    let context = context();
    let base = scene(
        &[
            [0, 0, 1_000],
            [0, 1_100, 1_000],
            [0, 1_700, 1_000],
            [0, 2_600, 1_000],
        ],
        600_000,
        Some([255, 255, 0, 0]),
    );
    let source = source_with(
        &super::terrain::terrain_scene(&base, false, false),
        SceneShadowCascades::new(vec![650, 1_200, 2_000, 3_500], 100_000).unwrap(),
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    for y in [0.0, 1.1, 1.7, 2.6] {
        assert_shadow(&image, &baseline, Vec3::new(-0.93, y, 0.0));
        assert_lit(&image, &baseline, Vec3::new(-0.57, y, 0.0));
    }
}

#[test]
fn cascade_contract_rejects_unrepresentable_ranges_before_upload() {
    for extents in [
        vec![1, 2, 3, 4, 5],
        vec![1, u64::MAX],
        vec![4_294_967_294, 4_294_967_295],
    ] {
        let effects = with_cascades(&fixture::effects(600_000), extents, 0);
        assert!(settings::validate_effects(&effects).is_err());
    }
    for splits in [vec![10, 100], vec![1_000, 30_000]] {
        let mut parts = fixture::effects_parts(600_000);
        parts.shadows = SceneShadowCascades::camera_splits(splits, 0).unwrap();
        assert!(settings::validate_effects(&SceneEffects::new(parts).unwrap()).is_err());
    }
    let mut parts = fixture::effects_parts(600_000);
    parts.shadows = SceneShadowCascades::camera_splits(vec![1_000, 2_000], 0).unwrap();
    assert!(
        DirectionalShadowSettings {
            resolution: 1,
            ..settings()
        }
        .validate_light(&SceneEffects::new(parts).unwrap())
        .is_err()
    );
}

#[test]
fn split_and_projection_distances_share_the_same_unit_conversion() {
    for far in [16_777_219, 33_554_433, u32::MAX] {
        let camera = SceneCamera {
            far_plane_millimeters: far,
            ..*fixture::effects(600_000).camera()
        };
        let view = SceneView::new(
            crate::CameraPose {
                eye_millimeters: [0, -1_000, 1_000],
                target_millimeters: [0, 0, 0],
            },
            camera,
            view().viewport(),
        )
        .unwrap();
        assert_eq!(
            super::super::cascades::meters(u64::from(far))
                .unwrap()
                .to_bits(),
            view.clip_distances_meters()[1].to_bits()
        );
    }
}
