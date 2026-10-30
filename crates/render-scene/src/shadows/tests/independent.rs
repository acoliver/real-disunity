use super::*;

#[test]
fn independent_shadow_default_matches_explicit_direct_sun_byte_for_byte() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let explicit = independent(&source, Some(source.effects().unwrap().sun().direction));
    let reset = independent(&explicit, None);
    let original = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    for unchanged in [explicit, reset] {
        assert_eq!(
            capture_scene_shadowed(&context, &unchanged, &view(), settings()).unwrap(),
            original
        );
    }
}

#[test]
fn independent_shadow_depth_fitting_uses_shadow_light_in_both_paths() {
    use crate::shadows::sdsm::AdoptionPolicy;
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let changed = independent(
        &source,
        Some(SceneDirection::new([-800_000, 0, 600_000]).unwrap()),
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    for policy in [
        AdoptionPolicy::SameFrameCapture,
        AdoptionPolicy::Interactive {
            latency_frames: 1,
            contraction_frames: 1,
        },
    ] {
        let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
        let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
        renderer
            .set_depth_fitting(&context, Some(depth_settings(policy)))
            .unwrap();
        render_target(&mut renderer, &context, &view(), &target).unwrap();
        assert_shadow(
            &target.read_rgba8(&context).unwrap(),
            &baseline,
            Vec3::new(-0.75, 0.0, 0.0),
        );
        renderer.set_effects(changed.effects().unwrap()).unwrap();
        render_target(&mut renderer, &context, &view(), &target).unwrap();
        let updated = target.read_rgba8(&context).unwrap();
        assert!(renderer.last_depth_fit().is_some());
        assert_shadow(&updated, &baseline, Vec3::new(4.0 / 3.0, 0.0, 0.0));
        assert_lit(&updated, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    }
}

fn depth_settings(
    policy: crate::shadows::sdsm::AdoptionPolicy,
) -> crate::shadows::sdsm::SdsmSettings {
    crate::shadows::sdsm::SdsmSettings {
        policy,
        range: worldrunner_render_shadow::RangeFit {
            near_step: 0.1,
            far_step: 1.0,
            minimum_span: 1.0,
            minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
            active_maximum: None,
            split_lambda: 0.5,
            split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
        },
    }
}

#[test]
fn independent_shadow_uses_validated_scene_directions() {
    for components in [[0; 3], [0, 0, 500_000], [i32::MAX; 3]] {
        assert_eq!(
            SceneDirection::new(components),
            Err(SceneValidationError::DirectionNotNormalized)
        );
    }
    // The lighting service accepts either hemisphere; scene directions preserve that policy.
    let below = SceneDirection::new([0, 0, -1_000_000]).unwrap();
    let source = independent(&scene(&[], 600_000, None), Some(below));
    assert_eq!(
        FrameUniform::new(&view(), source.effects())
            .unwrap()
            .shadow_direction,
        Vec3::NEG_Z
    );
}

fn independent(source: &RenderScene, direction: Option<SceneDirection>) -> RenderScene {
    source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(
                source
                    .effects()
                    .unwrap()
                    .clone()
                    .with_shadow_direction(direction),
            ),
        )
        .unwrap()
}

#[test]
fn independent_shadow_keeps_direct_uniform_and_default_bytes() {
    let source = scene(&[], 600_000, None);
    let effects = source.effects().unwrap();
    assert_eq!(effects.shadow_direction(), None);
    let original = FrameUniform::new(&view(), Some(effects)).unwrap();
    assert_eq!(original.shadow_direction, original.sun_direction);
    let changed = independent(
        &source,
        Some(SceneDirection::new([-800_000, 0, 600_000]).unwrap()),
    );
    let changed = FrameUniform::new(&view(), changed.effects()).unwrap();
    assert!(
        changed
            .shadow_direction
            .abs_diff_eq(Vec3::new(-0.8, 0.0, 0.6), 1e-6)
    );
    assert_eq!(changed.to_bytes(), original.to_bytes());
    let reset = independent(&source, None);
    assert_eq!(
        FrameUniform::new(&view(), reset.effects()).unwrap(),
        original
    );
}

#[test]
fn independent_shadow_moves_and_resizes_without_relighting_or_reupload() {
    let context = context();
    let source = vertical_caster();
    let changed = independent(
        &source,
        Some(SceneDirection::new([-800_000, 0, 600_000]).unwrap()),
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    assert_eq!(
        capture_scene(&context, &changed, &view()).unwrap(),
        baseline
    );
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let initial = target.read_rgba8(&context).unwrap();
    let handles: Vec<_> = renderer.scene_mut().instance_handles().collect();
    assert_projected_extent(&initial, &baseline, Vec3::new(0.6, 0.0, 0.8));
    renderer.set_effects(changed.effects().unwrap()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let updated = target.read_rgba8(&context).unwrap();
    assert_projected_extent(&updated, &baseline, Vec3::new(-0.8, 0.0, 0.6));
    assert_lit(&updated, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    assert_lit(&initial, &baseline, Vec3::new(4.0 / 3.0, 0.0, 0.0));
    assert_eq!(
        renderer.scene_mut().instance_handles().collect::<Vec<_>>(),
        handles
    );
    assert_eq!(
        capture_scene_shadowed(&context, &changed, &view(), settings()).unwrap(),
        updated
    );
    renderer.set_effects(source.effects().unwrap()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), initial);
}

fn assert_projected_extent(image: &RgbaImage, baseline: &RgbaImage, light: Vec3) {
    let project = |height| Vec3::new(0.0, 0.0, height) - height * light / light.z;
    let start = project(0.5);
    let end = project(1.5);
    // Stay beyond the existing PCF edge and normal-bias footprint on either side.
    let margin = (end - start).normalize() * 0.12;
    for point in [start + margin, start.lerp(end, 0.5), end - margin] {
        assert_shadow(image, baseline, point);
    }
    assert_lit(image, baseline, start - margin);
    assert_lit(image, baseline, end + margin);
}

fn vertical_caster() -> RenderScene {
    let vertices = [
        [0, -300, 500],
        [0, 300, 500],
        [0, 300, 1_500],
        [0, -300, 1_500],
    ]
    .into_iter()
    .map(|position| SceneVertex {
        position_millimeters: position,
        normal_micro: [1_000_000, 0, 0],
        tangent_micro: None,
        texcoord_micro: [0; 2],
    })
    .collect();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    fixture::caster_scene(vec![fixture::object([0; 3], 1, 1)], mesh, None, true)
}
