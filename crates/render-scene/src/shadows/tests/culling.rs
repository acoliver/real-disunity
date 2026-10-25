use super::*;

#[test]
fn gpu_offscreen_upstream_caster_is_culled_from_color_but_shadows_visible_ground() {
    let context = context();
    let source = scene(&[[1_500, 0, 2_000]], 600_000, None);
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
    let settings = DirectionalShadowSettings {
        depth_half_extent_meters: 0.5,
        ..settings()
    };
    let mut renderer = ShadowedScene::new(&context, &source, settings).unwrap();
    let caster = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    let shadow = target.read_rgba8(&context).unwrap();
    assert_eq!(renderer.view_stats().considered, 2);
    assert_eq!(renderer.view_stats().culled, 1);
    assert_eq!(renderer.view_stats().drawn_per_lod, vec![1]);
    assert_eq!(renderer.shadow_view_stats().drawn_per_lod, vec![2]);
    renderer
        .scene_mut()
        .set_instance_visible(caster, false)
        .unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    let baseline = target.read_rgba8(&context).unwrap();
    let point = Vec3::ZERO;
    assert!(
        i16::from(at_view(&baseline, point, &view)) - i16::from(at_view(&shadow, point, &view))
            > 40
    );
    assert_eq!(renderer.shadow_view_stats().drawn_per_lod, vec![1]);
    let plain = capture_scene(&context, &source, &view).unwrap();
    assert!(
        plain == capture_scene(&context, &scene(&[], 600_000, None), &view).unwrap(),
        "the offscreen caster must not contribute color pixels"
    );
    assert!(at_view(&baseline, point, &view).abs_diff(at_view(&plain, point, &view)) <= 1);
    renderer
        .scene_mut()
        .set_instance_visible(caster, true)
        .unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), shadow);
}
