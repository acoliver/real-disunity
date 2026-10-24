//! Real retained CLR/native session; primitive-field fixture observation only.
#![cfg(all(
    feature = "managed",
    feature = "scene",
    feature = "gfx",
    feature = "world"
))]

#[path = "managed_snapshot_consumer/controls.rs"]
mod controls;
#[path = "managed_snapshot_consumer/extraction.rs"]
mod extraction;
#[path = "managed_snapshot_consumer/journey.rs"]
mod journey;
#[path = "managed_snapshot_consumer/native.rs"]
mod native;
#[path = "managed_snapshot_consumer/observation.rs"]
mod observation;
#[path = "managed_snapshot_consumer/receipt.rs"]
mod receipt;
#[path = "managed_snapshot_consumer/render.rs"]
mod render;
#[path = "managed_snapshot_consumer/stage.rs"]
mod stage;
#[path = "managed_snapshot_consumer/store.rs"]
mod store;
#[path = "managed_snapshot_consumer/view.rs"]
mod view;

#[test]
fn retained_managed_snapshot_drives_canonical_gpu_capture() {
    use worldrunner::scene::gpu::{device::GpuContext, wgpu};
    let mut fixture = native::Fixture::boot();
    fixture.require_snapshots();
    let session = fixture.load();
    let original = extraction::Extraction::capture(&fixture, &session).unwrap();
    assert_eq!(original.stamp.frame, 0);
    assert_eq!(original.managed.fields()["updates"], 0);
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required retained managed GPU must be available");
    journey::run(&mut fixture, &session, &original, &context);
}
