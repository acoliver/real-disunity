//! Shadowed and reflective scenes write the same wind residual as the plain scene.
//!
//! The shadow receivers and the reflection forward pipelines are rewritten variants of the plain
//! object pipelines, so their residual is compared against the same CPU model.
use super::*;
use crate::shadows::{DirectionalShadowSettings, ShadowedScene};
use worldrunner_render_plan::{
    SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsr,
};

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

/// The scene with SSAO and temporal antialiasing authored, and no reflections.
pub(super) fn with_occlusion(scene: &RenderScene) -> (RenderScene, SceneEffects) {
    let effects = SceneEffects::new(SceneEffectsParts {
        anti_alias: SceneAntiAlias::taa(900_000),
        ssao: Some(SceneSsao {
            receiver: worldrunner_render_plan::SceneSsaoReceiver::Nearest,
            depth_footprint: SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            radius_millimeters: 2000,
            intensity_micro: 1_000_000,
            sample_count: 16,
            bias_millimeters: 10,
            horizon_angle_millidegrees: 90_000,
        }),
        ..effect_parts()
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
    shadowed_velocity_with(context, with_taa(scene), frames)
}

/// Draws a scene authored for TAA through a shadow receiver set and returns the last residual.
fn shadowed_velocity_with(
    context: &GpuContext,
    (scene, effects): (RenderScene, SceneEffects),
    frames: &[(f32, CameraPose)],
) -> Vec<[f32; 2]> {
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

/// Draws the scene with SSAO and no reflections through the SSAO velocity pipelines and returns
/// the last residual.
fn occluded_velocity_after(
    context: &GpuContext,
    scene: &RenderScene,
    frames: &[(f32, CameraPose)],
) -> Vec<[f32; 2]> {
    let (scene, effects) = with_occlusion(scene);
    let pipelines = ScenePipelines::with_ssao_taa_velocity(context.device(), HDR, 1).unwrap();
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

#[test]
fn an_ssao_scene_under_taa_writes_the_residual_of_the_plain_scene() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let frames = [(0.25, pose()), (0.3125, pose())];
    let plain = velocity_after(&context, &windy(), &frames);
    let occluded = occluded_velocity_after(&context, &windy(), &frames);
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_matches_plain(&occluded, &plain);
    assert_matches_cpu_residual(&occluded, (250.0, 312.5));
}

#[test]
fn shadows_reflections_and_sky_under_taa_write_the_residual_of_the_plain_scene() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let frames = [(0.125, pose()), (0.25, pose())];
    let plain = velocity_after(&context, &windy_under_sky(false), &frames);
    let combined =
        shadowed_velocity_with(&context, with_reflections(&windy_under_sky(false)), &frames);
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_matches_plain(&combined, &plain);
    assert_matches_cpu_residual(&combined, (125.0, 250.0));
    assert_sky_texels_are_zero_and_quad_moves(&combined);
}

#[test]
fn reflections_and_sky_under_taa_draw_without_validation_errors_and_keep_sky_velocity_zero() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let frames = [(0.125, pose()), (0.25, pose())];
    let reflective = reflective_velocity_after(&context, &windy_under_sky(false), &frames);
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_matches_cpu_residual(&reflective, (125.0, 250.0));
    assert_sky_texels_are_zero_and_quad_moves(&reflective);
}
