use super::*;

fn source(relief: bool, alpha: [u8; 4], enlarged: bool) -> RenderScene {
    let base = fixture::caster_scene(
        vec![fixture::object(
            if enlarged {
                [3000, 0, 4000]
            } else {
                [0, 0, 1000]
            },
            1,
            1,
        )],
        geometry::card(BillboardPolicy::YawLocked),
        Some(alpha),
        true,
    );
    let mut textures = base.resources().textures().to_vec();
    for value in [[128, 128, 0, 255], [255; 4], [0; 4]] {
        textures.push(SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: value.to_vec().into(),
        });
    }
    let mut atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    if relief {
        atlas = atlas.with_relief(TextureHandle::from_raw(3), 1000).unwrap();
    }
    if enlarged {
        atlas = atlas
            .with_plane_size([4000; 2])
            .unwrap()
            .with_borders(TextureHandle::from_raw(4));
    }
    let mut materials = base.resources().materials().to_vec();
    materials[1] = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: Some(TextureHandle::from_raw(2)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(atlas);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn camera() -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: [-4000, 0, 6500],
            target_millimeters: [0; 3],
        },
        *fixture::effects(600_000).camera(),
        view().viewport(),
    )
    .unwrap()
}

#[test]
fn gpu_relief_directional_shadows_use_flat_card_cutout_with_independent_views() {
    let context = context();
    let view = camera();
    let mut maps = Vec::new();
    for relief in [false, true] {
        let source = source(relief, [255, 255, 0, 0], false);
        let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
        let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        render_target(&mut renderer, &context, &view, &target).unwrap();
        let image = target.read_rgba8(&context).unwrap();
        let baseline = capture_scene(&context, &source, &view).unwrap();
        let shadow = Vec3::new(-0.75, 0.18, 0.0);
        let hole = Vec3::new(-0.75, -0.18, 0.0);
        assert!(
            i16::from(at_view(&baseline, shadow, &view))
                - i16::from(at_view(&image, shadow, &view))
                > 40
        );
        assert!(at_view(&baseline, hole, &view).abs_diff(at_view(&image, hole, &view)) <= 2);
        maps.push(refresh::depth_layers(&context, &renderer));
    }
    assert_eq!(
        maps[0], maps[1],
        "relief must change neither card raster depth nor cutout shadow"
    );
}

#[test]
fn gpu_enlarged_capture_card_extends_upstream_shadow_fit() {
    let context = context();
    let source = source(false, [255; 4], true);
    let view = camera();
    let baseline = capture_scene(&context, &source, &view).unwrap();
    let image = capture_scene_shadowed(
        &context,
        &source,
        &view,
        DirectionalShadowSettings {
            focus_meters: Vec3::new(-1.0, 0.0, 0.0),
            depth_half_extent_meters: 0.5,
            ..settings()
        },
    )
    .unwrap();
    // At x=-1 the sun ray hits the enlarged card at z=16/3, above the source mesh.
    let point = Vec3::new(-1.0, 0.0, 0.0);
    assert!(
        i16::from(at_view(&baseline, point, &view)) - i16::from(at_view(&image, point, &view)) > 40
    );
}
