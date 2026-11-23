use super::*;
mod texture_tiers;

struct QueuedCapture {
    id: u64,
    statistics: crate::RetainedFrameStatistics,
    center: u8,
}

#[test]
fn retained_configuration_replacement_preserves_captured_generations() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = prepass_host().windowless(&mut driver, None).unwrap();
    let mut captures = Vec::new();
    let mut previous_frame = None;
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
        assert_eq!(capture.statistics.scene.textures.resident_bytes, 4);
        assert_eq!(
            capture.statistics.scene.textures.uploaded_bytes,
            if generation == 0 { 4 } else { 0 }
        );
        assert_generation(&capture.statistics, generation);
        if let Some(previous) = previous_frame {
            assert!(capture.statistics.frame > previous);
        } else {
            assert_eq!(capture.statistics.frame, 0);
        }
        previous_frame = Some(capture.statistics.frame);
        assert!(!events.borrow().iter().any(|event| matches!(
            event, RetainedEvent::Capture { id, .. } if *id == capture.id
        )));
        captures.push(capture);
    }
    run.renderer.drain().unwrap();
    assert!(run.frame().unwrap());
    let steady = *run.host().statistics().unwrap();
    assert!(steady.frame > previous_frame.unwrap());
    assert_generation(&steady, 2);
    assert_eq!(steady.scene.textures.resident_bytes, 4);
    assert_eq!(steady.scene.textures.uploaded_bytes, 0);
    run.renderer.drain().unwrap();
    run.poll().unwrap();
    for capture in &captures {
        assert_completed_capture(&events.borrow(), capture);
    }
    run.shutdown().unwrap();
}

/// Each captured generation: its index, the reflection ray length in millimetres, the exposure
/// and the centre pixel. The ray length is part of what decides the stock plan, and a reflection
/// parameter change keeps the scene's resources, where switching reflections on or off, or any
/// SSAO change, would rebuild them. The quad is fully rough, at or above the reflection cutoff,
/// so reflections change no pixel. The middle generation is therefore planned anew without a new
/// upload, and the last one finds the first generation's plan. Reinhard maps unit radiance to 2/3
/// and 3/4, away from native sRGB rounding boundaries.
const GENERATIONS: [(u64, u32, u32, u8); 3] = [
    (0, 100_000, 2_000_000, 213),
    (1, 12_000, 3_000_000, 225),
    (2, 100_000, 2_000_000, 213),
];

fn prepass_host() -> RetainedHost {
    let base = crate::renderer::retained_tests::source();
    let effects = prepass_effects(base.effects().unwrap(), GENERATIONS[0].1, 2_000_000);
    let scene = base
        .with_objects_and_effects(base.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    let mut retained = host(scene);
    retained.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    let mut shadows = crate::renderer::retained_tests::shadow_settings();
    shadows.normal_bias_meters = 0.0;
    retained.options.shadows = Some(shadows);
    retained
}

fn prepass_effects(base: &SceneEffects, reach: u32, exposure: u32) -> SceneEffects {
    let mut grading = *base.grading();
    grading.exposure_micro = exposure;
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *base.sky(),
        sun: *base.sun(),
        ambient: *base.ambient(),
        shadows: base.shadows().clone(),
        // Zero strength reserves depth without changing the ambient-radiance oracle.
        ssao: Some(prepass_settings()),
        camera: *base.camera(),
        grading,
        post: vec![],
        anti_alias: *base.anti_alias(),
    })
    .unwrap()
    .with_ssr(worldrunner_render_plan::SceneSsr {
        max_distance_millimeters: reach,
        ..Default::default()
    })
    .unwrap()
}

fn prepass_settings() -> worldrunner_render_plan::SceneSsao {
    use worldrunner_render_plan::{
        SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver,
    };
    SceneSsao {
        receiver: SceneSsaoReceiver::Nearest,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        sample_count: 1,
        bias_millimeters: 0,
        radius_millimeters: 1,
        intensity_micro: 0,
        horizon_angle_millidegrees: 90_000,
    }
}

fn submit_capture(run: &mut WindowlessRetained<'_>, center: u8) -> QueuedCapture {
    let generation = run.host().accepted_generation();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8],
        1024 * 1024,
    )
    .unwrap();
    let id = run
        .host_mut()
        .request_capture_recipe(generation, recipe)
        .unwrap();
    let mut watchdog = CaptureWatchdog::new(run, format!("generation {generation}"));
    loop {
        let submitted = run.frame().unwrap();
        let statistics = run
            .host()
            .statistics()
            .copied()
            .filter(|statistics| submitted && statistics.generation == generation);
        watchdog.check(run, statistics.is_some());
        if let Some(statistics) = statistics {
            assert_eq!(run.host().ready_generation(), generation);
            return QueuedCapture {
                id,
                statistics,
                center,
            };
        }
    }
}

fn assert_generation(statistics: &crate::RetainedFrameStatistics, generation: u64) {
    assert_eq!(statistics.generation, generation);
    assert_eq!(statistics.scene.passes.opaque_objects.meshes.calls, 1);
    assert_eq!(statistics.scene.passes.opaque_objects.meshes.instances, 1);
    assert_eq!(statistics.scene.passes.prepass_objects.meshes.calls, 1);
    assert_eq!(statistics.scene.passes.prepass_objects.meshes.instances, 1);
    let meshes = statistics.scene.mesh_buffers;
    assert!(meshes.resident_bytes > 0);
    assert_eq!(
        meshes.uploaded_bytes,
        if generation == 0 {
            meshes.resident_bytes
        } else {
            0
        }
    );
    assert_eq!(
        statistics.scene.effects.shadows.installation,
        crate::PassInstallation::Installed
    );
    assert_eq!(
        statistics.scene.effects.shadows.receivers.meshes,
        crate::ReceiverBinding::Bound
    );
}

fn assert_completed_capture(events: &[RetainedEvent], capture: &QueuedCapture) {
    let (generation, frame, time, observation, image) = events
        .iter()
        .find_map(|event| match event {
            RetainedEvent::Capture {
                id,
                generation,
                outcome:
                    RetainedCaptureOutcome::RecipeCompleted {
                        frame,
                        time,
                        products,
                        observation,
                    },
            } if *id == capture.id => Some((
                generation,
                frame,
                time,
                observation,
                products.srgb8.as_ref().unwrap(),
            )),
            _ => None,
        })
        .expect("the submitted source capture completed");
    assert_eq!(*generation, capture.statistics.generation);
    assert_eq!(*frame, capture.statistics.frame);
    assert_eq!(*time, Duration::ZERO);
    assert_eq!(observation.generation, capture.statistics.generation);
    assert_eq!(observation.scene, capture.statistics.scene);
    assert_eq!(
        observation.temporal_history_reset,
        capture.statistics.temporal_history_reset
    );
    assert_pixels(image, capture.center);
}

fn assert_pixels(image: &RgbaImage, center: u8) {
    assert_eq!(
        image.viewport,
        worldrunner_render_scene::Viewport {
            width: 64,
            height: 64
        }
    );
    assert_eq!(image.pixels.len(), 64 * 64 * 4);
    let center_pixel = (32 * 64 + 32) * 4;
    assert_eq!(
        &image.pixels[center_pixel..center_pixel + 4],
        &[center, center, center, 255]
    );
    assert_eq!(&image.pixels[..4], &[0, 0, 0, 255]);
}
