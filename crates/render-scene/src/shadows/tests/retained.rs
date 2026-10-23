use super::*;

#[test]
fn resident_edits_reach_both_shadow_and_color_passes() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let initial = target.read_rgba8(&context).unwrap();
    assert_shadow(&initial, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    let handles: Vec<_> = renderer.scene_mut().instance_handles().collect();
    let occluder = handles[1];
    let original = *renderer
        .scene_mut()
        .instance(occluder)
        .unwrap()
        .0
        .transform();
    let moved = SceneTransform {
        position_millimeters: [100_000, 0, 1_000],
        ..original
    };
    renderer
        .scene_mut()
        .set_instance_transform(occluder, moved)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let relocated = target.read_rgba8(&context).unwrap();
    assert_lit(&relocated, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    renderer
        .scene_mut()
        .set_instance_transform(occluder, original)
        .unwrap();
    renderer
        .scene_mut()
        .set_instance_visible(occluder, false)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), relocated);
    renderer
        .scene_mut()
        .set_instance_visible(occluder, true)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(target.read_rgba8(&context).unwrap(), initial);
    assert!(
        renderer
            .scene_mut()
            .flush_instances(context.device(), context.queue())
            .is_empty()
    );
}
