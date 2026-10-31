use super::*;
use worldrunner_render_plan::*;
use worldrunner_render_scene::{Viewport, draw::PreparedScene};

fn source(side: u32, changed: bool) -> RenderScene {
    let base = super::super::retained_tests::terrain_source();
    let terrain = base.terrain().unwrap().clone().with_streaming_policy(Some(
        TerrainStreamingPolicy::new(if changed { 2 } else { 1 }, 9, 4, 1000).unwrap(),
    ));
    let cube = crate::presentation::preparation_tests::source(side);
    let mut textures = base.resources().textures().to_vec();
    textures.push(cube.resources().textures().last().unwrap().clone());
    let resources = SceneResources::new(textures, base.resources().materials().to_vec()).unwrap();
    let effects = base.effects().unwrap();
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *effects.sky(),
        sun: *effects.sun(),
        ambient: *effects.ambient(),
        shadows: effects.shadows().clone(),
        camera: *effects.camera(),
        grading: *effects.grading(),
        post: vec![],
        anti_alias: SceneAntiAlias::taa(900_000),
        ssao: None,
    })
    .unwrap();
    base.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
        .with_resources(resources, base.geometry().clone(), Some(effects))
        .unwrap()
}

#[test]
fn source_deferred_replacement_preserves_temporal_state() {
    check_deferred(true);
}

#[test]
fn source_deferred_replacement_keeps_old_cube_charged_until_retirement() {
    check_deferred(false);
}

fn check_deferred(check_presentation: bool) {
    let context = pollster::block_on(super::super::GpuContext::request()).unwrap();
    let old = source(128, false);
    let next = source(4, true);
    let (mut resident, mut presentation, mut host, profile) = setup(&context, &old, &next);
    assert!(presentation.scheduler.has_temporal_history());
    let generation = presentation.scheduler.generation();
    let color = presentation.targets.color.texture().clone();
    let limits = context.device().limits();
    let baseline = presentation.scheduler.resident_bytes(&limits).unwrap();
    host.apply(crate::RetainedUpdate::Resources {
        resources: next.resources().clone(),
        meshes: next.geometry().meshes().to_vec(),
    })
    .unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    send.send(Ok(PreparedScene::new(&next).unwrap())).unwrap();
    host.readiness.queue.front_mut().unwrap().scene = Some(receive);
    let mut streaming = Streaming::new(None, profile);
    streaming
        .publish(&context, &mut resident, &mut presentation, &mut host)
        .unwrap();
    assert_eq!(host.ready_generation(), 0);
    assert!(streaming.preparing.is_some());
    if check_presentation {
        assert_eq!(presentation.scheduler.generation(), generation);
        assert_eq!(presentation.targets.color.texture(), &color);
        assert!(presentation.scheduler.has_temporal_history());
    }
    assert_pending_bytes(&context, &presentation, baseline, &next);
    streaming
        .publish(&context, &mut resident, &mut presentation, &mut host)
        .unwrap();
    assert_eq!(host.ready_generation(), 1);
    assert_eq!(streaming.retired.len(), 1);
    let before = presentation.scheduler.resident_bytes(&limits).unwrap();
    streaming.collect(u64::MAX);
    let after = presentation.scheduler.resident_bytes(&limits).unwrap();
    let old_bytes = worldrunner_render_scene::draw::source_environment::resident_allocations(&old)
        .validate(&limits)
        .unwrap();
    assert_eq!(before - after, old_bytes);
}

fn assert_pending_bytes(
    context: &super::super::GpuContext,
    presentation: &Presentation,
    baseline: u64,
    next: &RenderScene,
) {
    let pending = worldrunner_render_scene::draw::source_environment::allocations(next)
        .unwrap()
        .validate(&context.device().limits())
        .unwrap();
    assert_eq!(
        presentation
            .scheduler
            .resident_bytes(&context.device().limits())
            .unwrap(),
        baseline + pending
    );
}

fn setup(
    context: &super::super::GpuContext,
    old: &RenderScene,
    next: &RenderScene,
) -> (Resident, Presentation, RetainedHost, crate::FrameProfile) {
    let profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    let host = RetainedHost::new(
        next.clone(),
        crate::ViewerOptions {
            size: [8, 8],
            frame_profile: profile,
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut presentation = Presentation::with_settings(
        context,
        crate::presentation::COLOR,
        [Viewport {
            width: 8,
            height: 8,
        }; 2],
        profile,
        old.effects(),
        &crate::presentation::PresentationPolicy {
            budget: crate::PresentationBudget(2 * 1024 * 1024).into(),
            recipes: false,
            capture_pool: crate::CapturePoolPolicy::default(),
        },
    )
    .unwrap();
    let mut resident =
        super::super::environment::new_resident(context, old, None, profile, &mut presentation)
            .unwrap();
    let mut target = worldrunner_render_scene::offscreen::OffscreenTarget::new(
        context.device(),
        Viewport {
            width: 8,
            height: 8,
        },
    )
    .unwrap();
    super::tests::draw(
        context,
        &mut resident,
        &mut presentation,
        &host,
        &mut target,
    );
    (resident, presentation, host, profile)
}
