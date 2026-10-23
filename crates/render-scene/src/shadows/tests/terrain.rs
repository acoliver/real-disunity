use super::*;

#[test]
fn gpu_off_camera_terrain_ridge_still_shadows_visible_valley() {
    let context = context();
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [-900, -4_000, 1_500],
            target_millimeters: [-900, 0, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 30_000,
            ..*fixture::effects(600_000).camera()
        },
        view().viewport(),
    )
    .unwrap();
    let source = terrain_scene(&scene(&[], 600_000, None), true, false);
    let ridge = view
        .view_projection_matrix()
        .project_point3(Vec3::new(1.0, 0.0, 3.0));
    assert!(ridge.x.abs() > 1.0 || ridge.y.abs() > 1.0);
    let baseline = capture_scene(&context, &source, &view).unwrap();
    let image = capture_scene_shadowed(
        &context,
        &source,
        &view,
        DirectionalShadowSettings {
            focus_meters: Vec3::new(-0.9, 0.0, 0.0),
            depth_half_extent_meters: 0.25,
            ..settings()
        },
    )
    .unwrap();
    let point = Vec3::new(-0.9, 0.0, 0.0);
    assert!(
        i16::from(at_view(&baseline, point, &view)) - i16::from(at_view(&image, point, &view)) > 40
    );
}

#[test]
fn gpu_masked_billboards_receive_terrain_shadows_and_preserve_holes() {
    let context = context();
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [4_000, 0, 10_000],
            target_millimeters: [-900, 0, 300],
        },
        *fixture::effects(600_000).camera(),
        view().viewport(),
    )
    .unwrap();
    let empty = terrain_scene(&scene(&[], 600_000, None), true, false);
    let ground = capture_scene_shadowed(&context, &empty, &view, settings()).unwrap();
    for policy in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
        let source = fixture::caster_scene(
            vec![fixture::object([-900, 0, 300], 1, 1)],
            super::geometry::card(policy),
            Some([255, 255, 0, 0]),
            true,
        );
        let source = terrain_scene(&source, true, false);
        let baseline = capture_scene(&context, &source, &view).unwrap();
        let image = capture_scene_shadowed(&context, &source, &view, settings()).unwrap();
        let solid = Vec3::new(-0.9, -0.18, 0.3);
        let delta =
            i16::from(at_view(&baseline, solid, &view)) - i16::from(at_view(&image, solid, &view));
        assert!(delta > 40, "{policy:?}: receiver delta={delta}");
        let hole = Vec3::new(-0.9, 0.18, 0.3);
        assert_eq!(
            at_view(&image, hole, &view),
            at_view(&ground, hole, &view),
            "{policy:?}"
        );
    }
}

fn terrain_scene(source: &RenderScene, ridge: bool, keep_ground: bool) -> RenderScene {
    let mut textures = source.resources().textures().to_vec();
    let normal = u32::try_from(textures.len()).unwrap();
    textures.extend(terrain_maps());
    let mut materials = source.resources().materials().to_vec();
    let material = u32::try_from(materials.len()).unwrap();
    materials.push(
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: Some(TextureHandle::from_raw(normal)),
            roughness_metallic: Some(TextureHandle::from_raw(normal + 1)),
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 1_000_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .unwrap(),
    );
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 7,
        rows: 7,
        origin_east_millimeters: -3_000,
        origin_north_millimeters: -3_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: (0..49)
            .map(|i| if ridge && i % 7 == 4 { 3_000 } else { 0 })
            .collect(),
        surface_indices: vec![0; 49],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(material),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(
            source.geometry().meshes().to_vec(),
            source
                .geometry()
                .objects()
                .iter()
                .skip(usize::from(!keep_ground))
                .copied()
                .collect(),
        )
        .unwrap(),
        terrain: Some(terrain),
        effects: source.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn gpu_terrain_receives_mesh_and_masked_foliage_shadows_with_holes() {
    let context = context();
    for alpha in [None, Some([255, 255, 0, 0])] {
        let source = terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, alpha), false, false);
        let baseline = capture_scene(&context, &source, &view()).unwrap();
        let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
        assert_shadow(&image, &baseline, Vec3::new(-0.93, 0.0, 0.0));
        if alpha.is_some() {
            assert_lit(&image, &baseline, Vec3::new(-0.57, 0.0, 0.0));
        } else {
            assert_shadow(&image, &baseline, Vec3::new(-0.57, 0.0, 0.0));
        }
        assert_lit(&image, &baseline, Vec3::new(-2.0, 0.0, 0.0));
    }
}

#[test]
fn gpu_terrain_ridge_casts_onto_valley_and_mesh_with_extended_light_depth() {
    let context = context();
    for objects in [vec![], vec![[-900, 0, 200]]] {
        let source = terrain_scene(&scene(&objects, 600_000, None), true, false);
        let baseline = capture_scene(&context, &source, &view()).unwrap();
        let image = capture_scene_shadowed(
            &context,
            &source,
            &view(),
            DirectionalShadowSettings {
                focus_meters: Vec3::new(-0.9, 0.0, 0.0),
                depth_half_extent_meters: 0.25,
                ..settings()
            },
        )
        .unwrap();
        let z = if objects.is_empty() { 0.0 } else { 0.2 };
        assert_shadow(&image, &baseline, Vec3::new(-0.9, 0.0, z));
        assert_lit(&image, &baseline, Vec3::new(-2.0, 0.0, 0.0));
    }
}

#[test]
fn gpu_terrain_shadow_path_preserves_retained_placement_edits() {
    let context = context();
    let source = terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), false, false);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let handle = renderer.scene_mut().instance_handles().next().unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let before = target.read_rgba8(&context).unwrap();
    renderer
        .scene_mut()
        .set_instance_visible(handle, false)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let hidden = target.read_rgba8(&context).unwrap();
    assert_shadow(&before, &hidden, Vec3::new(-0.75, 0.0, 0.0));
    renderer
        .scene_mut()
        .set_instance_visible(handle, true)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), before);
}

#[test]
fn gpu_masked_billboards_cast_holes_onto_terrain_in_camera_orientation() {
    let context = context();
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [-4_000, 0, 6_500],
            target_millimeters: [0, 0, 0],
        },
        *fixture::effects(600_000).camera(),
        view().viewport(),
    )
    .unwrap();
    for policy in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
        let source = fixture::caster_scene(
            vec![fixture::object([0, 0, 1_000], 1, 1)],
            super::geometry::card(policy),
            Some([255, 255, 0, 0]),
            true,
        );
        let source = terrain_scene(&source, false, false);
        let baseline = capture_scene(&context, &source, &view).unwrap();
        let image = capture_scene_shadowed(&context, &source, &view, settings()).unwrap();
        for (y, shadowed) in [(0.18, true), (-0.18, false)] {
            let point = Vec3::new(-0.75, y, 0.0);
            let delta = i16::from(at_view(&baseline, point, &view))
                - i16::from(at_view(&image, point, &view));
            assert!(
                if shadowed {
                    delta > 40
                } else {
                    delta.abs() <= 2
                },
                "{policy:?}, {point:?}, delta={delta}"
            );
        }
    }
}

#[test]
fn gpu_masked_mesh_receives_terrain_shadow_without_filling_coverage_holes() {
    let context = context();
    let source = terrain_scene(
        &scene(&[[-900, 0, 200]], 600_000, Some([255, 255, 0, 0])),
        true,
        false,
    );
    let empty = terrain_scene(&scene(&[], 600_000, None), true, false);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    let ground = capture_scene_shadowed(&context, &empty, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, Vec3::new(-1.03, 0.0, 0.2));
    assert_eq!(
        at(&image, Vec3::new(-0.72, 0.0, 0.2)),
        at(&ground, Vec3::new(-0.72, 0.0, 0.2))
    );
}

fn terrain_maps() -> impl Iterator<Item = SceneTexture> {
    [[128, 128, 255, 255], [0, 255, 0, 255]]
        .into_iter()
        .map(|bytes| SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: Vec::from(bytes).into(),
        })
}
