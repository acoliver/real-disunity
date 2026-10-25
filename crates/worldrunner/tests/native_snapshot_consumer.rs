//! Native snapshots drive the canonical renderer: extraction identity, stale-capture rejection,
//! resize and release.
#![cfg(all(
    feature = "managed",
    feature = "scene",
    feature = "gfx",
    feature = "world"
))]

#[path = "../../disunity-runtime/tests/support/handshake.rs"]
pub mod artifacts;
#[path = "native_snapshot_consumer/capture.rs"]
mod capture;
#[path = "native_snapshot_consumer/controls.rs"]
mod controls;
#[path = "native_snapshot_consumer/extraction.rs"]
mod extraction;
#[path = "native_snapshot_consumer/native.rs"]
mod native;
#[path = "native_snapshot_consumer/render.rs"]
mod render;
#[path = "native_snapshot_consumer/stage.rs"]
mod stage;
#[path = "native_snapshot_consumer/store.rs"]
mod store;
#[path = "native_snapshot_consumer/view.rs"]
mod view;

#[test]
fn native_snapshot_drives_canonical_gpu_capture() {
    use worldrunner::scene::gpu::{device::GpuContext, wgpu};
    let mut fixture = native::Fixture::boot();
    fixture.require_snapshots();
    let session = fixture.open();
    let object = fixture.initialize(&session);
    let original = extraction::Extraction::capture(&fixture, &session).unwrap();
    assert_eq!(original.stamp.world, session.world);
    controls::identities(&mut fixture, &session, &original);
    controls::copied_payload(&fixture, &session, &original);
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required native snapshot GPU must be available");
    let mut renderer = render::Renderer::new(&context);
    let first = renderer
        .capture(&fixture, &session, &original, object)
        .unwrap();
    first
        .bind_current(&fixture, &session, &original, &renderer.target)
        .unwrap();
    controls::markers(&first, &original, object);
    let fresh =
        controls::stale_after_mutation(&fixture, &session, &original, &mut renderer, object);
    assert!(
        first
            .bind_current(&fixture, &session, &fresh, &renderer.target)
            .is_err(),
        "old pixels cannot acquire fresh extraction identity"
    );
    let second = renderer
        .capture(&fixture, &session, &fresh, object)
        .unwrap();
    controls::markers(&second, &fresh, object);
    assert_ne!(first.gpu_completion_hash, second.gpu_completion_hash);
    assert_eq!(
        second.pixel(32, 23),
        [0, 0, 0, 255],
        "old marker position clears"
    );
    controls::retained_pixels(&context, &first);
    controls::resize(&fixture, &session, &fresh, &mut renderer, object, &second);
    controls::retained_pixels(&context, &second);
    controls::codec_rejects(&fixture, &session, object);
    original.release(&fixture, &session);
    controls::released_and_dead(&mut fixture, &session, &fresh, &mut renderer, object);
}
