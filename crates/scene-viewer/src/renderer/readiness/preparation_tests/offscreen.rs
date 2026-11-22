use crate::renderer::Renderer;
use crate::renderer::retained_tests::{
    offscreen::clearing_operations, shadow_settings, terrain_source,
};
use crate::{RetainedHost, RetainedUpdate};
use std::sync::Arc;
use worldrunner_render_plan::{SceneTerrain, SceneTerrainParts, TerrainStreamingPolicy};
use worldrunner_render_program::offscreen::PendingOffscreen;

#[test]
fn offscreen_preparation_pending_rejects_fitted_generation_without_frame_submission() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let source = terrain_source();
    let terrain = source
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    let updated_terrain = replacement(&terrain);
    let next = source
        .with_terrain(Some(Arc::new(updated_terrain.clone())))
        .unwrap();
    let initial = source
        .with_terrain(Some(Arc::new(terrain.clone())))
        .unwrap();
    let mut host = RetainedHost::new(
        initial,
        crate::ViewerOptions {
            shadows: Some(shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    host.apply(RetainedUpdate::Terrain(Some(Arc::new(
        updated_terrain.clone(),
    ))))
    .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    send.send(Ok(worldrunner_render_scene::draw::PreparedScene::new(
        &next,
    )
    .unwrap()))
        .unwrap();
    host.readiness.queue.front_mut().unwrap().scene = Some(receive);
    host.apply(RetainedUpdate::Settings(Box::new(
        crate::renderer::sdsm_tests::fitted_scene()
            .effects()
            .unwrap()
            .clone(),
    )))
    .unwrap();

    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _image, _runtime) = clearing_operations(&device, &authority);
    host.admit_offscreen(operations);
    let packet = std::ptr::from_ref(host.offscreen.as_ref().unwrap());
    let serial = authority.submitted_serial();
    let capture_frame = renderer.streaming.captures.next_frame();

    assert_preparation_pending(&mut renderer, &mut host, packet, serial, capture_frame);
    renderer.drain().unwrap();
    assert_fitted_rejection(&mut renderer, &mut host, packet, capture_frame);
}

fn assert_preparation_pending(
    renderer: &mut Renderer,
    host: &mut RetainedHost,
    packet: *const PendingOffscreen,
    serial: u64,
    capture_frame: u64,
) {
    assert!(matches!(renderer.render_retained(host), Ok(false)));
    // The new resident flushes its instances, then preparation uploads one terrain batch.
    assert_eq!(renderer.context.submission().submitted_serial(), serial + 2);
    assert_eq!(host.accepted_generation(), 2);
    assert_eq!(host.readiness.published, 0);
    assert_eq!(host.readiness.queue.front().unwrap().generation, 1);
    assert!(renderer.streaming.preparing.is_some());
    assert!(std::ptr::eq(packet, host.offscreen.as_ref().unwrap()));
    assert_eq!(renderer.streaming.captures.next_frame(), capture_frame);
    assert!(!renderer.streaming.captures.is_pending());
}

fn assert_fitted_rejection(
    renderer: &mut Renderer,
    host: &mut RetainedHost,
    packet: *const PendingOffscreen,
    capture_frame: u64,
) {
    let serial = renderer.context.submission().submitted_serial();
    assert!(matches!(
        renderer.render_retained(host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert_eq!(host.ready_generation(), 2);
    assert!(renderer.resident.depth_fitting_enabled());
    assert_eq!(renderer.context.submission().submitted_serial(), serial);
    assert!(std::ptr::eq(packet, host.offscreen.as_ref().unwrap()));
    assert_eq!(renderer.streaming.captures.next_frame(), capture_frame);
    assert!(!renderer.streaming.captures.is_pending());
}

fn replacement(base: &SceneTerrain) -> SceneTerrain {
    let mut heights = base.heights_millimeters().to_vec();
    heights[0] += 1;
    SceneTerrain::new(SceneTerrainParts {
        columns: base.columns(),
        rows: base.rows(),
        origin_east_millimeters: base.origin_east_millimeters(),
        origin_north_millimeters: base.origin_north_millimeters(),
        sample_spacing_millimeters: base.sample_spacing_millimeters(),
        heights_millimeters: heights,
        surface_indices: base.surface_indices().to_vec(),
        surfaces: base.surfaces().to_vec(),
        lod_policy: base.lod_policy(),
    })
    .unwrap()
    .with_streaming_policy(base.streaming_policy())
}
