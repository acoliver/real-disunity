use super::*;
use worldrunner_render_plan::SceneDirection;

#[test]
fn gpu_unsupported_effect_update_preserves_the_active_profile() {
    let context = context();
    let source = with_effects(sun_z());
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let before = retained_image(&context, &mut renderer, &mut target);
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(worldrunner_render_plan::SceneSsao {
        sample_count: 16,
        bias_millimeters: 10,
        radius_millimeters: 500,
        intensity_micro: 0,
        horizon_angle_millidegrees: 30_000,
    });
    assert!(matches!(
        renderer.set_effects(&SceneEffects::new(parts).unwrap()),
        Err(ShadowError::Frame(FrameError::UnsupportedEffect("SSAO")))
    ));
    assert_eq!(before, retained_image(&context, &mut renderer, &mut target));
}

fn translated_fit() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        focus_meters: Vec3::new(16_777_216.0, 0.0, 0.0),
        depth_half_extent_meters: 1.0,
        half_extent_meters: 4.0,
        ..settings()
    }
}

fn axis_effects(direction: SceneDirection) -> SceneEffects {
    let mut parts = fixture::effects_parts(600_000);
    parts.sun.direction = direction;
    SceneEffects::new(parts).unwrap()
}

fn sun_x() -> SceneEffects {
    axis_effects(SceneDirection::new([1_000_000, 0, 0]).unwrap())
}

fn sun_z() -> SceneEffects {
    axis_effects(SceneDirection::new([0, 0, 1_000_000]).unwrap())
}

fn with_effects(effects: SceneEffects) -> RenderScene {
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

#[test]
fn translated_focus_fit_is_light_dependent_at_f32_precision_boundary() {
    let fit = translated_fit();
    fit.validate(&wgpu::Limits::default()).unwrap();
    let matrix = fit.matrix(Vec3::Z).unwrap();
    assert!(
        matrix
            .project_point3(fit.focus_meters)
            .abs_diff_eq(Vec3::new(0.0, 0.0, 0.5), 1e-5)
    );
    assert!(matches!(
        fit.matrix(Vec3::X),
        Err(ShadowError::InvalidSettings)
    ));
}

#[test]
fn gpu_construction_rejects_invalid_active_light_fit() {
    let context = context();
    let source = with_effects(sun_x());
    assert!(matches!(
        ShadowedScene::new(&context, &source, translated_fit()),
        Err(ShadowError::InvalidSettings)
    ));
    assert!(matches!(
        capture_scene_shadowed(&context, &source, &view(), translated_fit()),
        Err(ShadowError::InvalidSettings)
    ));
    assert!(SceneDirection::new([0; 3]).is_err());
}

fn retained_image(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
    target: &mut OffscreenTarget,
) -> RgbaImage {
    render_target(renderer, context, &view(), target).unwrap();
    target.read_rgba8(context).unwrap()
}

#[test]
fn gpu_rejected_active_light_fit_updates_leave_rendering_unchanged() {
    let context = context();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let mut renderer = ShadowedScene::new(&context, &with_effects(sun_x()), settings()).unwrap();
    let before = retained_image(&context, &mut renderer, &mut target);
    assert!(matches!(
        renderer.set_settings(
            &context,
            DirectionalShadowSettings {
                resolution: 1024,
                ..translated_fit()
            }
        ),
        Err(ShadowError::InvalidSettings)
    ));
    assert_eq!(renderer.settings, settings());
    assert_eq!(before, retained_image(&context, &mut renderer, &mut target));

    let mut renderer =
        ShadowedScene::new(&context, &with_effects(sun_z()), translated_fit()).unwrap();
    let before = retained_image(&context, &mut renderer, &mut target);
    assert!(matches!(
        renderer.set_effects(&sun_x()),
        Err(ShadowError::InvalidSettings)
    ));
    assert_eq!(before, retained_image(&context, &mut renderer, &mut target));
}
