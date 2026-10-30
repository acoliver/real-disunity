mod native_rcas;

#[test]
fn depth_fitted_recipe_then_ordinary_frame_and_plain_capture_keep_frame_identity() {
    let mut host = RetainedHost::new(
        crate::renderer::sdsm_tests::fitted_scene(),
        crate::ViewerOptions {
            size: [64, 64],
            shadows: Some(crate::renderer::retained_tests::shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let id = host
        .request_capture_recipe(0, recipe(1, 2, CaptureJitter::Halton23))
        .unwrap();
    for frame in 0..2 {
        assert!(!renderer.render_retained(&mut host).unwrap());
        assert_eq!(renderer.streaming.captures.next_frame(), frame + 1);
        renderer.drain().unwrap();
        renderer.collect_retained(&mut host).unwrap();
    }
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture {
        id: actual, outcome: RetainedCaptureOutcome::RecipeCompleted { frame: 0, .. }, ..
    }) if actual == id)
    );
    assert!(renderer.render_retained(&mut host).unwrap());
    assert_eq!(renderer.streaming.captures.next_frame(), 3);
    renderer.drain().unwrap();
    let plain = host.request_capture(0).unwrap();
    assert!(renderer.render_retained(&mut host).unwrap());
    assert_eq!(renderer.streaming.captures.next_frame(), 4);
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture {
        id: actual, outcome: RetainedCaptureOutcome::Completed { frame: 3, .. }, ..
    }) if actual == plain)
    );
    let crate::renderer::Resident::Shadowed(scene) = &renderer.resident else {
        panic!("shadow resident");
    };
    let fit = scene.last_depth_fit().unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (3, 3));
}

#[test]
fn retained_hdr_reserves_recipe_storage_at_startup_and_preserves_it_across_resize() {
    let (mut host, _) = setup();
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let inventory = |renderer: &Renderer| {
        renderer
            .presentation
            .scheduler
            .configuration()
            .additional
            .validate(&renderer.context.device().limits())
            .unwrap()
    };
    let reserved = inventory(&renderer);
    let generation = renderer.presentation.scheduler.generation();
    let ordinary = 4 * 256 * 64 + 3 * 64 * 64 * 4;
    assert_eq!(reserved, ordinary + 256 * 1024 * 1024);
    host.request_capture_recipe(0, recipe(1, 2, CaptureJitter::None))
        .unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(inventory(&renderer), ordinary + 256 * 1024 * 1024);
    assert_eq!(renderer.presentation.scheduler.generation(), generation);
    renderer.drain().unwrap();
    renderer
        .presentation
        .resize(
            &renderer.context,
            Viewport {
                width: 96,
                height: 64,
            },
            Viewport {
                width: 96,
                height: 64,
            },
        )
        .unwrap();
    assert_eq!(
        inventory(&renderer),
        4 * 512 * 64 + 3 * 96 * 64 * 4 + 256 * 1024 * 1024
    );
}

use super::*;
use crate::{
    CaptureJitter, CaptureOutput, RenderClock, RetainedCaptureOutcome, RetainedCaptureProducts,
    RetainedEvent,
};

fn setup() -> (RetainedHost, Renderer) {
    let host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    renderer.set_clock(RenderClock::Fixed(Duration::ZERO));
    (host, renderer)
}
fn recipe(factor: u32, samples: u32, jitter: CaptureJitter) -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        factor,
        samples,
        jitter,
        false,
        vec![CaptureOutput::Srgb8],
        4 * 1024 * 1024,
    )
    .unwrap()
}
fn completed(
    host: &mut RetainedHost,
    renderer: &mut Renderer,
    recipe: RetainedCaptureRecipe,
) -> RetainedCaptureProducts {
    let id = host.request_capture_recipe(0, recipe).unwrap();
    await_capture(host, renderer, id)
}
fn await_capture(
    host: &mut RetainedHost,
    renderer: &mut Renderer,
    id: u64,
) -> RetainedCaptureProducts {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        renderer.render_retained(host).unwrap();
        renderer.poll().unwrap();
        renderer.collect_retained(host).unwrap();
        if let Some(RetainedEvent::Capture {
            id: actual,
            outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
            ..
        }) = host.captures.take_event()
        {
            assert_eq!(id, actual);
            return products;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn direct(renderer: &mut Renderer, view: &SceneView, time: Duration) -> RetainedCaptureProducts {
    let size = view.viewport();
    let ready = renderer
        .execution
        .acquire_capture(&renderer.context, [size.width, size.height])
        .unwrap()
        .unwrap();
    renderer
        .presentation
        .resize(&renderer.context, size, size)
        .unwrap();
    renderer.resident.select_frame_slot(ready.admission.slot());
    renderer
        .presentation
        .scheduler
        .select_frame_slot(ready.admission.slot());
    renderer
        .presentation
        .scheduler
        .set_frame_time_duration(time)
        .unwrap();
    let output = ready
        .frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut image = Products::new(
        renderer.context.device(),
        &ready.frame.texture,
        renderer.presentation.scheduler.targets().texture(),
        &[CaptureOutput::Srgb8],
        None,
    )
    .unwrap();
    let prepared = renderer
        .encode_frame(view, None, &output, Some(&mut image))
        .unwrap();
    renderer
        .execution
        .submit_capture(ready, prepared)
        .unwrap()
        .wait(renderer.context.device())
        .unwrap();
    renderer.streaming.captures.advance_frame();
    image.begin().unwrap();
    loop {
        renderer.poll().unwrap();
        if let Some(products) = image.try_read().unwrap() {
            return products;
        }
        std::thread::yield_now();
    }
}
#[test]
fn supersample_two_equals_exact_box_of_double_resolution_render() {
    let (mut host, mut renderer) = setup();
    let actual = completed(&mut host, &mut renderer, recipe(2, 1, CaptureJitter::None));
    let large = host
        .view()
        .unwrap()
        .unwrap()
        .with_raster_sample(
            Viewport {
                width: 128,
                height: 128,
            },
            [0.0; 2],
        )
        .unwrap();
    let reference = direct(&mut renderer, &large, Duration::ZERO).srgb8.unwrap();
    let mut expected = Vec::new();
    for y in 0..64 {
        for x in 0..64 {
            for c in 0..4 {
                let mut sum = 0_u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        sum +=
                            u32::from(reference.pixels[((y * 2 + dy) * 128 + x * 2 + dx) * 4 + c]);
                    }
                }
                expected.push(u8::try_from((sum + 2) / 4).unwrap());
            }
        }
    }
    assert_eq!(actual.srgb8.unwrap().pixels, expected);
}
#[test]
fn temporal_is_deterministic_and_uses_declared_projection_sequence() {
    let (mut host, mut renderer) = setup();
    let recipe = recipe(1, 4, CaptureJitter::Halton23);
    let first = completed(&mut host, &mut renderer, recipe.clone());
    assert_eq!(first, completed(&mut host, &mut renderer, recipe.clone()));
    let view = host.view().unwrap().unwrap();
    let mut sum = Accumulation::new(view.viewport(), &recipe);
    for (sample, expected) in [
        [0.0, -1.0 / 6.0],
        [-0.25, 1.0 / 6.0],
        [0.25, -7.0 / 18.0],
        [-0.375, -1.0 / 18.0],
    ]
    .into_iter()
    .enumerate()
    {
        let jitter = recipe.jitter_offset(u32::try_from(sample).unwrap());
        for c in 0..2 {
            assert!((jitter[c] - expected[c]).abs() < 0.000_001);
        }
        let shifted = view.with_raster_sample(view.viewport(), jitter).unwrap();
        let point = glam::Vec3::ZERO;
        let before = view.resolved_view().world_to_screen(point).unwrap();
        let after = shifted.resolved_view().world_to_screen(point).unwrap();
        assert!((after.x - before.x - jitter[0]).abs() < 0.000_01);
        assert!((after.y - before.y - jitter[1]).abs() < 0.000_01);
        sum.add(&direct(&mut renderer, &shifted, Duration::ZERO));
    }
    assert_eq!(first, sum.finish());
}
#[test]
fn cancellation_during_accumulation_preserves_request_and_drops_no_completion() {
    let (mut host, mut renderer) = setup();
    let id = host
        .request_capture_recipe(0, recipe(2, 4, CaptureJitter::Halton23))
        .unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    assert!(renderer.streaming.captures.active.is_some());
    assert!(host.captures.waiting.iter().any(|r| r.0 == id));
    host.captures.cancel();
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(renderer.streaming.captures.active.is_none());
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. }) if actual == id)
    );
    assert!(host.captures.take_event().is_none());
}
#[test]
fn subframe_clock_is_fixed_or_advances_at_declared_sixty_hertz() {
    let fixed = recipe(1, 4, CaptureJitter::None);
    let mut moving = fixed.clone();
    moving.advance_clock = true;
    let base = Duration::from_secs(3);
    for sample in 0..4 {
        assert_eq!(fixed.subframe_time(base, sample), base);
        assert_eq!(
            moving.subframe_time(base, sample),
            base + Duration::from_nanos(u64::from(sample) * 1_000_000_000 / 60)
        );
    }
}

#[test]
fn hdr_upscaled_accumulation_uses_full_resolution_and_restores_plain_render_scale() {
    let (mut host, _) = setup();
    host.options.frame_profile = crate::FrameProfile::HdrUpscaled {
        samples: 4,
        tone_map: crate::ToneMap::Reinhard,
        scale: crate::RenderScale::new(500_000).unwrap(),
        upscale: crate::UpscaleSettings {
            mode: crate::UpscaleMode::Easu,
            sharpness: None,
        },
    };
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    renderer.set_clock(RenderClock::Fixed(Duration::ZERO));
    let recipe = RetainedCaptureRecipe::new(
        2,
        2,
        CaptureJitter::Halton23,
        false,
        vec![CaptureOutput::Srgb8, CaptureOutput::LinearHdr],
        4 * 1024 * 1024,
    )
    .unwrap();
    let result = completed(&mut host, &mut renderer, recipe);
    assert_eq!(
        result.linear_hdr.unwrap().viewport,
        Viewport {
            width: 64,
            height: 64
        }
    );
    host.request_capture(0).unwrap();
    renderer.render_retained(&mut host).unwrap();
    assert_eq!(renderer.presentation.targets.color.texture().width(), 32);
}

fn windy_scene() -> worldrunner_render_plan::RenderScene {
    use worldrunner_render_plan::*;
    let base = crate::renderer::retained_tests::source();
    let material = base.resources().materials()[0]
        .with_features(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: Some(
                SceneWind::new(SceneDirection::new([1_000_000, 0, 0]).unwrap(), 500, 100, 0)
                    .unwrap(),
            ),
            layer: None,
            translucency: None,
        })
        .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}
#[test]
fn advancing_clock_matches_rendered_subframe_times_and_changes_wind() {
    let mut host = RetainedHost::new(
        windy_scene(),
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    renderer.set_clock(RenderClock::Fixed(Duration::ZERO));
    let fixed = recipe(1, 4, CaptureJitter::None);
    let mut moving = fixed.clone();
    moving.advance_clock = true;
    let baseline = completed(&mut host, &mut renderer, fixed);
    let result = completed(&mut host, &mut renderer, moving.clone());
    let view = host.view().unwrap().unwrap();
    let mut expected = Accumulation::new(view.viewport(), &moving);
    for sample in 0..4 {
        expected.add(&direct(
            &mut renderer,
            &view,
            moving.subframe_time(Duration::ZERO, sample),
        ));
    }
    assert_eq!(result, expected.finish());
    assert_ne!(result, baseline);
    assert_eq!(result, completed(&mut host, &mut renderer, moving));
}

#[test]
fn temporal_accumulation_pins_generation_until_last_sample() {
    let (mut host, mut renderer) = setup();
    host.request_capture_recipe(0, recipe(1, 4, CaptureJitter::Halton23))
        .unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    host.apply(crate::RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    })
    .unwrap();
    for _ in 0..3 {
        renderer.drain().unwrap();
        renderer.poll().unwrap();
        renderer.collect_retained(&mut host).unwrap();
        assert_eq!(host.ready_generation(), 0);
        assert!(host.captures.holds_generation(0));
        assert!(!renderer.render_retained(&mut host).unwrap());
    }
    renderer.drain().unwrap();
    renderer.poll().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(!host.captures.holds_generation(0));
    renderer.render_retained(&mut host).unwrap();
    assert_eq!(host.ready_generation(), 1);
}

#[test]
fn accumulated_capture_freezes_left_handed_camera_across_queued_edit_and_resize() {
    let mut options = crate::tests::options();
    options.size = [64, 64];
    options.render_clock = RenderClock::Fixed(Duration::ZERO);
    options.camera_convention = worldrunner_camera::ViewConvention {
        handedness: worldrunner_camera::ViewHandedness::DirectXLeftHanded,
    };
    let mut host =
        RetainedHost::new(crate::renderer::retained_tests::terrain_source(), options).unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let fixed = recipe(2, 2, CaptureJitter::None);
    let reference = completed(&mut host, &mut renderer, fixed.clone());
    host.request_capture_recipe(0, fixed).unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    host.apply(crate::RetainedUpdate::Camera {
        pose: host.options.pose,
        lens: host.options.camera,
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
    })
    .unwrap();
    host.input(crate::RetainedInput::Resized([48, 32]));
    assert!(!renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    let Some(RetainedEvent::Capture {
        generation: 0,
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
        ..
    }) = host.captures.take_event()
    else {
        panic!("fixed capture");
    };
    assert_eq!(products, reference);
    assert_eq!(
        products.srgb8.unwrap().viewport,
        Viewport {
            width: 64,
            height: 64
        }
    );
    assert_eq!(host.ready_generation(), 0);
}
