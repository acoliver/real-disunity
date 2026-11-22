//! Accumulated captures take part in the game screen's bookkeeping as ordinary frames do: the
//! host's screen is offered to each sample, the queued operations that write it run in the first
//! submitted sample, and the sample's submission counts as the write.
use super::*;
use crate::GameScreen;
use crate::renderer::retained_tests::offscreen::screen_operations;
use worldrunner_render_scene::offscreen::RgbaImage;

const YELLOW: [u8; 4] = [255, 255, 0, 255];

fn is_all(image: &RgbaImage, expected: [u8; 4]) -> bool {
    image.pixels.chunks_exact(4).all(|pixel| pixel == expected)
}

/// Submits ordinary frames until one is accepted, then waits for the GPU.
fn ordinary_frame(host: &mut RetainedHost, renderer: &mut Renderer) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !renderer.render_retained(host).unwrap() {
        renderer.poll().unwrap();
        assert!(std::time::Instant::now() < deadline, "frame timed out");
        std::thread::yield_now();
    }
    renderer.drain().unwrap();
}

#[test]
fn an_accumulated_capture_runs_the_queued_screen_writer_and_shows_the_screen() {
    let (mut host, mut renderer) = setup();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(operations);

    let image = completed(&mut host, &mut renderer, recipe(2, 1, CaptureJitter::None))
        .srgb8
        .unwrap();

    assert!(
        host.offscreen.is_none(),
        "the capture's first submission took the queued writer"
    );
    assert!(
        is_all(&image, YELLOW),
        "the screen the writer wrote covers the stock scene"
    );
}

#[test]
fn every_temporal_sample_shows_the_screen_the_first_sample_wrote() {
    let (mut host, mut renderer) = setup();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(operations);

    let image = completed(&mut host, &mut renderer, recipe(1, 3, CaptureJitter::None))
        .srgb8
        .unwrap();

    assert!(
        is_all(&image, YELLOW),
        "an average of samples is yellow only if each sample showed the screen"
    );
}

#[test]
fn an_accumulated_capture_does_not_show_a_screen_the_host_withdrew() {
    let (mut host, mut renderer) = setup();
    let baseline = completed(&mut host, &mut renderer, recipe(2, 1, CaptureJitter::None))
        .srgb8
        .unwrap();
    assert!(!is_all(&baseline, YELLOW), "the stock scene is not yellow");

    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(operations);
    ordinary_frame(&mut host, &mut renderer);
    host.show_game_screen(None);

    let after = completed(&mut host, &mut renderer, recipe(2, 1, CaptureJitter::None))
        .srgb8
        .unwrap();

    assert_eq!(after, baseline, "no screen is offered: the stock scene");
}

/// Change the streaming grid so the next generation has to upload a replacement root chunk.
/// Supply the prepared CPU scene directly to remove worker scheduling from the assertion.
fn screen_preparation_fixture() -> (RetainedHost, Renderer) {
    use std::sync::Arc;
    use worldrunner_render_plan::TerrainStreamingPolicy;
    use worldrunner_render_scene::draw::PreparedScene;

    let source = crate::renderer::retained_tests::terrain_source();
    let terrain = |chunk_cells| {
        Arc::new(
            source
                .terrain()
                .unwrap()
                .clone()
                .with_streaming_policy(Some(
                    TerrainStreamingPolicy::new(chunk_cells, 9, 4, 1000).unwrap(),
                )),
        )
    };
    let mut host = RetainedHost::new(
        source.with_terrain(Some(terrain(1))).unwrap(),
        crate::ViewerOptions {
            size: [64, 64],
            ..crate::tests::options()
        },
    )
    .unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    let terrain = terrain(2);
    let next = source.with_terrain(Some(terrain.clone())).unwrap();
    host.apply(crate::RetainedUpdate::Terrain(Some(terrain)))
        .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    send.send(Ok(PreparedScene::new(&next).unwrap())).unwrap();
    host.readiness.queue.front_mut().unwrap().scene = Some(receive);
    (host, renderer)
}

#[test]
fn preparation_preserves_the_screen_writer_for_the_first_capture_sample() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut host, mut renderer) = screen_preparation_fixture();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(operations);
    let id = host
        .request_capture_recipe(1, recipe(2, 1, CaptureJitter::None))
        .unwrap();
    let before = authority.submitted_serial();
    let frame = renderer.streaming.captures.next_frame();

    assert!(
        !renderer.render_retained(&mut host).unwrap(),
        "preparation displayed a frame"
    );
    // The replacement flushes its instances and uploads one terrain batch.
    assert_eq!(authority.submitted_serial(), before + 2);
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(renderer.streaming.captures.next_frame(), frame);
    assert!(
        host.offscreen.is_some(),
        "preparation took the screen writer"
    );
    assert!(host.captures.waiting.iter().any(|request| request.0 == id));
    renderer.drain().unwrap();

    assert!(!renderer.render_retained(&mut host).unwrap());
    let active = renderer.streaming.captures.active.as_ref().unwrap();
    assert_eq!((active.id, active.sample, active.frame), (id, 0, frame));
    assert!(
        active.pending.is_some(),
        "first frame did not submit the sample"
    );
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    let Some(RetainedEvent::Capture {
        id: actual,
        generation: 1,
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
    }) = host.captures.take_event()
    else {
        panic!("first sample did not complete")
    };
    assert_eq!(actual, id);
    let image = products.srgb8.unwrap();
    assert_eq!(host.ready_generation(), 1);
    assert!(host.offscreen.is_none());
    assert_eq!(image.pixels.len(), 64 * 64 * 4);
    assert!(
        is_all(&image, YELLOW),
        "first sample missed the screen write"
    );
}

#[test]
fn screen_warmup_restores_the_missing_page_and_the_capture_completes() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut host, mut renderer, id, view) = capture_with_invalidated_page();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (ops, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(ops);
    let before = authority.submitted_serial();
    let warmup_frame = renderer.streaming.captures.next_frame();

    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(authority.submitted_serial(), before + 1);
    assert!(host.offscreen.is_none());
    let active = renderer.streaming.captures.active.as_ref().unwrap();
    assert_eq!((active.id, active.sample), (id, 0));
    assert!(
        active.pending.is_none(),
        "warm-up must not count as a sample"
    );
    renderer.drain().unwrap();
    assert!(
        renderer.view_pages_ready(&view).unwrap(),
        "screen warm-up left its required page missing"
    );

    assert!(!renderer.render_retained(&mut host).unwrap());
    let first_scene = renderer.scene_statistics();
    let first_reset = renderer.temporal_history_reset();
    let first_time = renderer.frame_time;
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    let Some(RetainedEvent::Capture {
        id: actual,
        generation: 0,
        outcome:
            RetainedCaptureOutcome::RecipeCompleted {
                frame,
                time,
                observation,
                products,
            },
    }) = host.captures.take_event()
    else {
        panic!("two samples after warmup did not complete");
    };
    assert_eq!(actual, id);
    assert_eq!(frame, warmup_frame + 1);
    assert_eq!(time, first_time);
    assert_eq!(observation.scene, first_scene);
    assert_eq!(observation.temporal_history_reset, first_reset);
    let image = products.srgb8.unwrap();
    assert_eq!(image.pixels.len(), 64 * 64 * 4);
    assert!(is_all(&image, YELLOW));
}

#[test]
fn an_ordinary_screen_frame_makes_pages_ready_for_a_queued_capture() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut host, mut renderer) = primed_page_scene();
    let view = invalidate_only_page(&host, &mut renderer);
    let id = host
        .request_capture_recipe(0, recipe(1, 2, CaptureJitter::None).with_page_dependency())
        .unwrap();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (ops, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(ops);
    assert!(renderer.render_retained(&mut host).unwrap());
    assert!(renderer.streaming.captures.active.is_none());
    assert!(host.captures.waiting.iter().any(|r| r.0 == id));
    renderer.drain().unwrap();
    assert!(
        renderer.view_pages_ready(&view).unwrap(),
        "ordinary screen frame left the queued capture's page missing"
    );
    let image = await_capture(&mut host, &mut renderer, id).srgb8.unwrap();
    assert_eq!(image.pixels.len(), 64 * 64 * 4);
    assert!(is_all(&image, YELLOW));
}
