//! Shadowed and reflective scenes write the same wind residual as the plain scene.
//!
//! The shadow receivers and the reflection forward pipelines are rewritten variants of the plain
//! object pipelines, so their residual is compared against the same CPU model.
use super::*;
use crate::shadows::{DirectionalShadowSettings, ShadowedScene};
use worldrunner_render_plan::SceneSsr;

fn shadow_settings() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        focus_meters: Vec3::new(0.0, 0.0, 0.5),
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 256,
        depth_bias: 0.001,
        normal_bias_meters: 0.01,
        refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
    }
}

fn view_from(pose: CameraPose) -> SceneView {
    SceneView::new(
        pose,
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap()
}

/// The windy scene with reflections requested beside its temporal antialiasing.
fn with_reflections(scene: &RenderScene) -> (RenderScene, SceneEffects) {
    let (scene, effects) = with_taa(scene);
    let effects = effects
        .with_ssr(SceneSsr {
            enabled: true,
            max_steps: 64,
            thickness_millimeters: 100,
            roughness_cutoff_micro: 600_000,
            resolution_scale_micro: 500_000,
            ..Default::default()
        })
        .unwrap();
    let scene = RenderScene::new(SceneParts {
        resources: scene.resources().clone(),
        geometry: scene.geometry().clone(),
        terrain: None,
        effects: Some(effects.clone()),
    })
    .unwrap();
    (scene, effects)
}

fn last_velocity(context: &GpuContext, renderer: &FrameRenderer) -> Vec<[f32; 2]> {
    let texture = renderer
        .targets()
        .velocity_texture()
        .expect("a TAA renderer allocates the velocity target")
        .clone();
    read_velocity(context, &texture)
}

/// Draws the scene through a shadow receiver set built for TAA and returns the last residual.
fn shadowed_velocity_after(
    context: &GpuContext,
    scene: &RenderScene,
    frames: &[(f32, CameraPose)],
) -> Vec<[f32; 2]> {
    let (scene, effects) = with_taa(scene);
    let mut shadowed =
        ShadowedScene::with_taa_velocity(context, &scene, shadow_settings(), HDR, 1).unwrap();
    let (mut renderer, color, depth) = taa_target(context, &effects, MsaaSamples::X1);
    for &(seconds, pose) in frames {
        renderer.set_frame_time(seconds).unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        shadowed
            .encode_with(
                context,
                &view_from(pose),
                (&color, &depth),
                &mut encoder,
                Some(&mut renderer),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
    }
    last_velocity(context, &renderer)
}

/// Draws the scene with reflections on and returns the last residual.
fn reflective_velocity_after(
    context: &GpuContext,
    scene: &RenderScene,
    frames: &[(f32, CameraPose)],
) -> Vec<[f32; 2]> {
    let (scene, effects) = with_reflections(scene);
    let pipelines = ScenePipelines::with_samples(context.device(), HDR, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let (mut renderer, color, depth) = taa_target(context, &effects, MsaaSamples::X1);
    for &(seconds, pose) in frames {
        renderer.set_frame_time(seconds).unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        crate::compositor::encode_scene(
            &mut renderer,
            context.device(),
            &mut encoder,
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame_from(pose),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
    }
    last_velocity(context, &renderer)
}

fn assert_matches_plain(velocity: &[[f32; 2]], plain: &[[f32; 2]]) {
    let mut written = 0;
    for (got, want) in velocity.iter().zip(plain) {
        if want[0].abs() > 1e-6 || want[1].abs() > 1e-6 {
            written += 1;
            assert!(
                (got[0] - want[0]).abs() < TOLERANCE_UV && (got[1] - want[1]).abs() < TOLERANCE_UV,
                "{got:?} against the plain scene's {want:?}"
            );
        }
    }
    assert!(written >= 200, "only {written} texels carry a residual");
}

#[test]
fn a_shadowed_scene_under_taa_writes_the_residual_of_the_plain_scene() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let frames = [(0.25, pose()), (0.3125, pose())];
    let plain = velocity_after(&context, &windy(), &frames);
    let shadowed = shadowed_velocity_after(&context, &windy(), &frames);
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_matches_plain(&shadowed, &plain);
    assert_matches_cpu_residual(&shadowed, (250.0, 312.5));
}

#[test]
fn a_reflective_scene_under_taa_writes_the_residual_of_the_plain_scene() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let frames = [(0.25, pose()), (0.3125, pose())];
    let plain = velocity_after(&context, &windy(), &frames);
    let reflective = reflective_velocity_after(&context, &windy(), &frames);
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_matches_plain(&reflective, &plain);
    assert_matches_cpu_residual(&reflective, (250.0, 312.5));
}
