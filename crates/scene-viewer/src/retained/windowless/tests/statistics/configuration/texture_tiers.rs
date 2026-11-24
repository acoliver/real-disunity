use super::*;
use worldrunner_render_gpu::census::ResourceClass;
use worldrunner_render_plan::{
    SceneResources, SceneTexture, TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::texture::{TextureTierPolicy, TierChoice};

#[test]
fn retained_reduced_texture_survives_capture_reconfiguration() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut host = prepass_host();
    install_reducible_texture(&mut host);
    let mut run = host.windowless(&mut driver, None).unwrap();
    let mut captures = Vec::new();
    let mut plans = Vec::new();
    let mut mesh_residency = None;
    for (generation, reach, exposure, center) in GENERATIONS {
        let prior = run.host().statistics().copied();
        if generation != 0 {
            let effects = prepass_effects(run.host().scene().effects().unwrap(), reach, exposure);
            run.host_mut()
                .apply(RetainedUpdate::Settings(Box::new(effects)))
                .unwrap();
            assert_eq!(run.host().statistics().copied(), prior);
        }
        assert_eq!(run.host().accepted_generation(), generation);
        let capture = submit_capture(&mut run, center);
        assert_eq!(capture.statistics.generation, generation);
        let mesh_bytes =
            *mesh_residency.get_or_insert(capture.statistics.scene.mesh_buffers.resident_bytes);
        assert_tier_generation(&capture.statistics, generation, mesh_bytes);
        assert_generation(&capture.statistics, generation);
        assert_eq!(run.host().ready_generation(), generation);
        assert!(!events.borrow().iter().any(|event| matches!(
            event, RetainedEvent::Capture { id, .. } if *id == capture.id
        )));
        plans.push(run.renderer.stock_plan_statistics());
        assert!(captures.last().is_none_or(|previous: &QueuedCapture| {
            capture.statistics.frame > previous.statistics.frame
        }));
        captures.push(capture);
    }
    assert_stock_plan_reused_after_reconfiguration(&plans);
    warm_every_frame_slot(&mut run);
    let before_steady = run.renderer.stock_plan_statistics();
    assert!(run.frame().unwrap());
    let steady = *run.host().statistics().unwrap();
    let after_steady = run.renderer.stock_plan_statistics();
    assert_stock_plan_reused_for_steady_frame(before_steady, after_steady);
    assert!(steady.frame > captures.last().unwrap().statistics.frame);
    assert_eq!(steady.generation, 2);
    assert_tier_generation(&steady, 2, mesh_residency.unwrap());
    assert_generation(&steady, 2);
    run.renderer.drain().unwrap();
    run.poll().unwrap();
    for capture in &captures {
        assert_completed_capture(&events.borrow(), capture);
    }
    run.shutdown().unwrap();
}

/// The renderer configuration admits at most three frame slots.
const MOST_FRAME_SLOTS: usize = 3;

/// The first frame in each frame slot resolves that slot's binding table. Frames in the settled
/// configuration run until every slot has been used, each one finished before the next, so the
/// frame the test measures finds its slot's table whichever slot the device completion order
/// gives it.
fn warm_every_frame_slot(run: &mut WindowlessRetained<'_>) {
    for _ in 0..2 * MOST_FRAME_SLOTS {
        assert!(run.frame().unwrap());
        run.renderer.drain().unwrap();
        run.poll().unwrap();
    }
}

fn install_reducible_texture(host: &mut RetainedHost) {
    let original = host.scene().clone();
    let mut textures = original.resources().textures().to_vec();
    textures[0] = seven_level_texture();
    let scene = original
        .with_resources(
            SceneResources::new(textures, original.resources().materials().to_vec()).unwrap(),
            original.geometry().clone(),
            original.effects().cloned(),
        )
        .unwrap()
        .with_reducible_textures(vec![TextureHandle::from_raw(0)])
        .unwrap();
    host.scene = Some(scene.clone());
    host.initial_scene = Some(scene);
    host.options.texture_tiers = Some(TextureTierPolicy {
        budget_fraction_micro: 1_000_000,
        max_dropped_levels: 2,
    });
    host.texture_tier_memory_override = Some(1);
}

fn seven_level_texture() -> SceneTexture {
    let mut bytes = Vec::with_capacity(21_844);
    for (level, color) in [
        [255, 0, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
    ]
    .into_iter()
    .enumerate()
    {
        let side = 64_usize >> level;
        bytes.extend(std::iter::repeat_n(color, side * side).flatten());
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 64,
        height: 64,
        mips: 7,
        bytes: bytes.into(),
    }
}

fn assert_tier_generation(
    statistics: &crate::RetainedFrameStatistics,
    generation: u64,
    mesh_bytes: u64,
) {
    assert_eq!(statistics.scene.textures.resident_bytes, 1_364);
    assert_eq!(
        statistics.scene.textures.uploaded_bytes,
        if generation == 0 { 1_364 } else { 0 }
    );
    let choice = statistics.texture_tier.unwrap();
    assert_eq!(
        choice,
        TierChoice {
            authored_bytes: 21_844,
            kept_bytes: 1_364,
            dropped_levels: 2,
            fits: false,
        }
    );
    assert_eq!(choice.saved_bytes(), 20_480);
    assert_eq!(statistics.resident_bytes.get(ResourceClass::Texture), 1_364);
    assert_eq!(
        statistics.resident_bytes.total(),
        statistics
            .resident_bytes
            .classes()
            .map(|(_, bytes)| bytes)
            .sum::<u64>()
    );
    assert!(statistics.resident_bytes.get(ResourceClass::RenderTarget) > 0);
    assert!(statistics.resident_bytes.get(ResourceClass::Instance) > 0);
    assert_eq!(statistics.scene.mesh_buffers.resident_bytes, mesh_bytes);
    assert_eq!(
        statistics.scene.mesh_buffers.uploaded_bytes,
        if generation == 0 { mesh_bytes } else { 0 }
    );
}

fn assert_stock_plan_reused_after_reconfiguration(
    plans: &[worldrunner_render_scene::compositor::StockPlanStatistics],
) {
    let [initial, changed, restored] = plans else {
        panic!("one stock-plan snapshot per captured generation");
    };
    assert!(initial.rebuilds > 0);
    assert!(initial.binding_resolutions > 0);
    assert!(initial.binding > 0);
    assert_eq!(changed.rebuilds, initial.rebuilds + 1);
    assert!(changed.binding_resolutions > initial.binding_resolutions);
    assert_eq!(restored.rebuilds, changed.rebuilds);
    assert!(restored.binding_resolutions > changed.binding_resolutions);
    assert_ne!(restored.binding, changed.binding);
}

fn assert_stock_plan_reused_for_steady_frame(
    before: worldrunner_render_scene::compositor::StockPlanStatistics,
    after: worldrunner_render_scene::compositor::StockPlanStatistics,
) {
    assert_eq!(after.rebuilds, before.rebuilds);
    assert_eq!(after.binding_resolutions, before.binding_resolutions);
    assert_eq!(after.reuses, before.reuses + 1);
    assert!(after.binding > 0);
}
