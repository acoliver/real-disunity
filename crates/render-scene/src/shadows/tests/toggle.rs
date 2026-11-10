use super::*;

#[test]
fn gpu_shadow_capture_requested_while_disabled_waits_for_enabled_submission() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let camera = view();
    let baseline = capture_scene(&context, &source, &camera).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, retained_settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let receiver = Vec3::new(-0.75, 0.0, 0.0);
    assert_shadow(
        &render_image(&mut renderer, &context, &camera, &mut target),
        &baseline,
        receiver,
    );
    let initial_refresh = renderer.cascade_refresh_states()[0].last_frame();
    take_shadow_execution();
    renderer.set_shadows_enabled(false);
    renderer.request_shadow_capture();
    let disabled = render_image(&mut renderer, &context, &camera, &mut target);
    assert_lit(&disabled, &baseline, receiver);
    assert!(!take_shadow_execution());
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_frame(),
        initial_refresh
    );

    renderer.set_shadows_enabled(true);
    let mut abandoned = context.submission().begin(context.device()).unwrap();
    renderer
        .encode(
            &context,
            &camera,
            &target.color_attachment(),
            &target.depth_attachment(),
            &mut abandoned,
        )
        .unwrap();
    drop(abandoned);
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_frame(),
        initial_refresh
    );
    take_shadow_execution();
    let restored = render_image(&mut renderer, &context, &camera, &mut target);
    assert_shadow(&restored, &baseline, receiver);
    assert!(take_shadow_execution());
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(worldrunner_render_shadow::RefreshReason::Capture)
    );
}

#[test]
fn gpu_resident_shadow_toggle_disables_and_restores_cached_shadow() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let camera = view();
    let baseline = capture_scene(&context, &source, &camera).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, retained_settings()).unwrap();
    assert!(renderer.shadows_enabled());
    let mut target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let warmed = render_image(&mut renderer, &context, &camera, &mut target);
    let receiver = Vec3::new(-0.75, 0.0, 0.0);
    assert_shadow(&warmed, &baseline, receiver);
    let initial_refresh = renderer.cascade_refresh_states()[0].last_frame();
    take_shadow_execution();

    renderer.set_shadows_enabled(false);
    assert!(!renderer.shadows_enabled());
    let disabled = render_image(&mut renderer, &context, &camera, &mut target);
    assert_lit(&disabled, &baseline, receiver);
    assert!(
        !take_shadow_execution(),
        "a cached map is not refreshed while shadows are disabled"
    );
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_frame(),
        initial_refresh,
        "disabling shadows leaves the warmed map untouched"
    );

    renderer.set_shadows_enabled(true);
    assert!(renderer.shadows_enabled());
    let restored = render_image(&mut renderer, &context, &camera, &mut target);
    assert_shadow(&restored, &baseline, receiver);
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_frame(),
        initial_refresh,
        "reenabling reuses the still-valid cached map"
    );
    assert!(!take_shadow_execution());
}

#[test]
fn gpu_shadow_toggle_before_first_frame_skips_maps_until_enabled() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let camera = view();
    let baseline = capture_scene(&context, &source, &camera).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let receiver = Vec3::new(-0.75, 0.0, 0.0);
    take_shadow_execution();

    renderer.set_shadows_enabled(false);
    let disabled = render_image(&mut renderer, &context, &camera, &mut target);
    assert_lit(&disabled, &baseline, receiver);
    assert!(!take_shadow_execution());
    assert!(
        renderer
            .cascade_refresh_states()
            .iter()
            .all(|state| state.last_frame().is_none())
    );

    renderer.set_shadows_enabled(true);
    let enabled = render_image(&mut renderer, &context, &camera, &mut target);
    assert_shadow(&enabled, &baseline, receiver);
    assert!(take_shadow_execution());
    assert!(
        renderer
            .cascade_refresh_states()
            .iter()
            .any(|state| state.last_frame().is_some())
    );
}

#[test]
fn gpu_shadow_toggle_refreshes_a_caster_moved_while_disabled() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let camera = view();
    let baseline = capture_scene(&context, &source, &camera).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, retained_settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let old_shadow = Vec3::new(-0.75, 0.0, 0.0);
    let new_shadow = Vec3::new(0.75, 0.0, 0.0);
    let warmed = render_image(&mut renderer, &context, &camera, &mut target);
    assert_shadow(&warmed, &baseline, old_shadow);
    assert_lit(&warmed, &baseline, new_shadow);
    let initial_refresh = renderer.cascade_refresh_states()[0].last_frame();
    let handles: Vec<_> = renderer.scene_mut().instance_handles().collect();
    take_shadow_execution();

    renderer.set_shadows_enabled(false);
    move_caster(&mut renderer);
    for _ in 0..2 {
        let disabled = render_image(&mut renderer, &context, &camera, &mut target);
        assert_lit(&disabled, &baseline, old_shadow);
        assert_lit(&disabled, &baseline, new_shadow);
        assert!(!take_shadow_execution());
        assert_eq!(
            renderer.cascade_refresh_states()[0].last_frame(),
            initial_refresh
        );
        assert_eq!(
            renderer.scene_mut().instance_handles().collect::<Vec<_>>(),
            handles
        );
    }

    renderer.set_shadows_enabled(true);
    let restored = render_image(&mut renderer, &context, &camera, &mut target);
    assert_lit(&restored, &baseline, old_shadow);
    assert_shadow(&restored, &baseline, new_shadow);
    assert_ne!(
        renderer.cascade_refresh_states()[0].last_frame(),
        initial_refresh
    );
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(worldrunner_render_shadow::RefreshReason::Invalidated)
    );
    assert!(take_shadow_execution());
    assert_eq!(
        renderer.scene_mut().instance_handles().collect::<Vec<_>>(),
        handles
    );
}

fn render_image(
    renderer: &mut ShadowedScene,
    context: &GpuContext,
    camera: &SceneView,
    target: &mut OffscreenTarget,
) -> RgbaImage {
    render_target(renderer, context, camera, target).unwrap();
    target.read_rgba8(context).unwrap()
}

fn move_caster(renderer: &mut ShadowedScene) {
    let caster = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let transform = *renderer.scene_mut().instance(caster).unwrap().0.transform();
    renderer
        .scene_mut()
        .set_instance_transform(
            caster,
            SceneTransform {
                position_millimeters: [1_500, 0, 1_000],
                ..transform
            },
        )
        .unwrap();
}

fn take_shadow_execution() -> bool {
    crate::scene_metadata::EXECUTED_PASSES
        .with_borrow_mut(std::mem::take)
        .iter()
        .any(|slot| matches!(slot, PassSlot::ShadowCasters(_)))
}

fn retained_settings() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy {
            intervals: [8; 4],
            phases: [0; 4],
            max_non_near: 3,
            light_delta_radians: 0.1,
            projection_delta_texels: 4.0,
        },
        ..settings()
    }
}
