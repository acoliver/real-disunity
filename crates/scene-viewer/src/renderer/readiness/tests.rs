#[test]
fn baked_effect_edits_prepare_then_publish_through_the_shared_renderer() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let source = super::super::retained_tests::source();
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let profile = FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    let mut host = RetainedHost::new(
        source.clone(),
        crate::ViewerOptions {
            size: [64, 64],
            frame_profile: profile,
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut resident = Resident::new(&context, &source, None, profile).unwrap();
    let mut presentation = Presentation::with_profile(
        &context,
        crate::presentation::COLOR,
        size,
        size,
        profile,
        source.effects(),
    )
    .unwrap();
    let mut streaming = Streaming::new(None, profile);
    let mut target = OffscreenTarget::new(context.device(), size).unwrap();
    for effects in baked_effects() {
        host.apply(RetainedUpdate::Settings(Box::new(effects.clone())))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while host.ready_generation() != host.accepted_generation() {
            context.device().poll(wgpu::PollType::Poll).unwrap();
            streaming
                .publish(&context, &mut resident, &mut presentation, &mut host)
                .unwrap();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_configured(&presentation, &effects);
        draw(
            &context,
            &mut resident,
            &mut presentation,
            &host,
            &mut target,
        );
    }
}

fn baked_effects() -> Vec<SceneEffects> {
    let base = crate::tests::scene(false).effects().unwrap().clone();
    let ssao = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *base.sky(),
        sun: *base.sun(),
        ambient: *base.ambient(),
        shadows: base.shadows().clone(),
        camera: *base.camera(),
        grading: *base.grading(),
        post: vec![],
        anti_alias: *base.anti_alias(),
        ssao: Some(SceneSsao {
            receiver: worldrunner_render_plan::SceneSsaoReceiver::Nearest,
            depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            sample_count: 4,
            bias_millimeters: 1,
            radius_millimeters: 100,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }),
    })
    .unwrap();
    let ssr = base
        .with_ssr(SceneSsr {
            enabled: true,
            max_steps: 8,
            traversal: SceneSsrTraversal::Grid,
            grid_domain: SceneSsrGridDomain::FullResolution,
            grid_level_count: None,
            max_distance_millimeters: 42_000,
            thickness_slope_micro: 20_000,
            origin_skip_pixels: 4,
            pass_behind_steps: 8,
            depth_relative_ao: true,
            roughness_pass_behind: None,
            roughness_fade_start_micro: Some(200_000),
            luminance_limit_milli: Some(2000),
            composition: SceneSsrComposition::ReplaceEnvironment,
            thickness_millimeters: 10,
            roughness_cutoff_micro: 1_000_000,
            roughness_cutoff_disabled: false,
            resolution_scale_micro: 500_000,
            radiance_rounding: SceneDownsampleRounding::Ceil,
            application_lod: SceneSsrLodMapping::CutoffNormalized,
            radiance_pyramid: SceneSsrPyramid::SixLevels,
        })
        .unwrap();
    vec![
        ssao,
        ssr,
        crate::tests::scene(true).effects().unwrap().clone(),
    ]
}

use super::*;
use crate::{FrameProfile, RetainedUpdate};
use worldrunner_render_plan::*;
use worldrunner_render_scene::Viewport;
use worldrunner_render_scene::{
    device::GpuContext,
    offscreen::{OffscreenTarget, RgbaImage},
};

pub(super) fn draw(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    host: &RetainedHost,
    target: &mut OffscreenTarget,
) -> RgbaImage {
    let view = host.view().unwrap().unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            resident.render(
                context,
                &view,
                &presentation.targets,
                encoder,
                presentation.scheduler.encoding(),
            )?;
            presentation.present(encoder, target.color_view())?;
            Ok::<_, ViewerError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    target.read_rgba8(context).unwrap()
}

#[test]
fn pending_preparation_keeps_rendering_then_publishes_whole_generations() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let source = super::super::retained_tests::source();
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let mut host = host(&source);
    let mut resident = Resident::new(&context, &source, None, FrameProfile::Legacy).unwrap();
    let mut presentation = Presentation::new(&context, crate::presentation::COLOR, size);
    let mut streaming = Streaming::new(None, FrameProfile::Legacy);
    let mut target = OffscreenTarget::new(context.device(), size).unwrap();
    let before_view = host.view().unwrap().unwrap().view_matrix();
    let textures = host.scene.resources().textures().as_ptr();
    let meshes = host.scene.geometry().meshes().as_ptr();
    let send = enqueue_replacement(&mut host, &source);
    let replacement_textures = host.scene.resources().textures().as_ptr();
    let replacement_meshes = host.scene.geometry().meshes().as_ptr();
    assert_ne!(textures, replacement_textures);
    assert_ne!(meshes, replacement_meshes);
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
    let initial = keep_previous_generation(
        &context,
        &mut resident,
        &mut presentation,
        &mut host,
        &mut target,
        &mut streaming,
    );
    let old_submission = context.submission().submitted_serial();
    send.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while host.ready_generation() == 0 {
        context.device().poll(wgpu::PollType::Poll).unwrap();
        streaming
            .publish(&context, &mut resident, &mut presentation, &mut host)
            .unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
    assert_replaced(&mut streaming, &host, old_submission);
    assert_ne!(
        draw(
            &context,
            &mut resident,
            &mut presentation,
            &host,
            &mut target
        ),
        initial
    );
    context.device().poll(wgpu::PollType::Poll).unwrap();
    streaming
        .publish(&context, &mut resident, &mut presentation, &mut host)
        .unwrap();
    assert_eq!(host.ready_generation(), 2);
    assert_ne!(host.view().unwrap().unwrap().view_matrix(), before_view);
    assert!(streaming.retired.is_empty());
}

fn enqueue_replacement(
    host: &mut RetainedHost,
    source: &RenderScene,
) -> std::sync::mpsc::SyncSender<()> {
    let mut textures = source.resources().textures().to_vec();
    textures[0] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 0, 0, 255].into_boxed_slice(),
    };
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    host.readiness.preparation_hook = Some(Box::new(move || receive.recv().unwrap()));
    host.apply(RetainedUpdate::Resources {
        resources: SceneResources::new(textures, source.resources().materials().to_vec()).unwrap(),
        meshes: source.geometry().meshes().to_vec(),
    })
    .unwrap();
    let mut options = crate::tests::options();
    options.pose.eye_millimeters[0] += 10_000;
    host.apply(RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: options.pose,
        lens: options.camera,
    })
    .unwrap();
    send
}

fn host(source: &RenderScene) -> RetainedHost {
    RetainedHost::with_preparation_limit(
        source.clone(),
        crate::ViewerOptions {
            size: [64, 64],
            ..crate::tests::options()
        },
        2,
    )
    .unwrap()
}

fn assert_replaced(streaming: &mut Streaming, host: &RetainedHost, old_submission: u64) {
    assert_eq!(host.ready_generation(), 1);
    streaming.collect(old_submission - 1);
    assert_eq!(streaming.retired.len(), 1);
    assert_eq!(streaming.retired[0].0, old_submission);
}

fn assert_configured(presentation: &Presentation, effects: &SceneEffects) {
    assert_eq!(
        presentation.scheduler.configuration().ssao.as_ref(),
        effects.ssao()
    );
    assert_eq!(
        presentation.scheduler.configuration().ssr.as_ref(),
        effects.ssr()
    );
    assert_eq!(
        presentation.scheduler.configuration().water,
        effects.water_bodies()
    );
}

#[test]
fn asynchronous_rebuilds_complete_in_reverse_but_publish_in_order() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let source = super::super::retained_tests::source();
    let mut host = host(&source);
    let mut resident = Resident::new(&context, &source, None, FrameProfile::Legacy).unwrap();
    let mut presentation = Presentation::new(
        &context,
        crate::presentation::COLOR,
        Viewport {
            width: 64,
            height: 64,
        },
    );
    let mut streaming = Streaming::new(None, FrameProfile::Legacy);
    let (release, gate) = std::sync::mpsc::sync_channel(1);
    host.readiness.preparation_hook = Some(Box::new(move || gate.recv().unwrap()));
    for _ in 0..2 {
        host.apply(RetainedUpdate::Resources {
            resources: source.resources().clone(),
            meshes: source.geometry().meshes().to_vec(),
        })
        .unwrap();
    }
    let second = host.readiness.queue[1]
        .scene
        .as_ref()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    send.send(Ok(second)).unwrap();
    host.readiness.queue[1].scene = Some(receive);
    streaming
        .publish(&context, &mut resident, &mut presentation, &mut host)
        .unwrap();
    assert_eq!(host.ready_generation(), 0);
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while host.ready_generation() == 0 {
        context.device().poll(wgpu::PollType::Poll).unwrap();
        streaming
            .publish(&context, &mut resident, &mut presentation, &mut host)
            .unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(host.ready_generation(), 1);
    assert_eq!(host.preparations_in_flight(), 1);
    streaming
        .publish(&context, &mut resident, &mut presentation, &mut host)
        .unwrap();
    assert_eq!(host.ready_generation(), 2);
}

fn keep_previous_generation(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    host: &mut RetainedHost,
    target: &mut OffscreenTarget,
    streaming: &mut Streaming,
) -> RgbaImage {
    let initial = draw(context, resident, presentation, host, target);
    let before_view = host.view().unwrap().unwrap().view_matrix();
    let replacement_textures = host.scene.resources().textures().as_ptr();
    let replacement_meshes = host.scene.geometry().meshes().as_ptr();
    for _ in 0..3 {
        context.device().poll(wgpu::PollType::Poll).unwrap();
        streaming
            .publish(context, resident, presentation, host)
            .unwrap();
        assert_eq!(host.ready_generation(), 0);
        assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
        assert_eq!(
            host.scene.resources().textures().as_ptr(),
            replacement_textures
        );
        assert_eq!(host.scene.geometry().meshes().as_ptr(), replacement_meshes);
        assert_eq!(draw(context, resident, presentation, host, target), initial);
    }
    initial
}
