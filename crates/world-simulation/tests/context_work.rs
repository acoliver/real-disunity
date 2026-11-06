//! The work a scriptable render context records: culling results, validation, call order and the
//! bytes that carry it.
use worldrunner_world_simulation::graphics::snapshot_wire::{
    WIRE_VERSION, WireError, decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, MAX_CULL_PLANES, OffscreenWork, TargetRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, ObjectDesc, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn camera(world: &mut World) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn planes(count: usize) -> Vec<[f32; 4]> {
    (0..count)
        .map(|index| {
            let step = f32::from(u8::try_from(index).unwrap());
            [step, -1.0, 0.25 * step, 1.0e-3 - step]
        })
        .collect()
}

fn cull(id: u64, camera: ComponentHandle, planes: Vec<[f32; 4]>) -> OffscreenWork {
    OffscreenWork::Cull {
        id,
        camera,
        culling_mask: 0x8000_0101,
        planes,
        options: 5,
    }
}

/// One record of each context kind for a world that holds a camera.
fn every_context_record(camera: ComponentHandle) -> Vec<OffscreenWork> {
    vec![cull(1, camera, planes(6)), cull(2, camera, Vec::new())]
}

#[test]
fn a_culling_result_is_recorded_and_taken_in_call_order() {
    let mut world = world();
    let camera = camera(&mut world);
    let records = every_context_record(camera);
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.offscreen_work(), records.as_slice());
    assert_eq!(world.take_offscreen(), records);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_culling_result_needs_a_live_camera() {
    let mut world = world();
    let live = camera(&mut world);
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let not_camera = world
        .add_component(object, ComponentDesc::AudioListener { enabled: true })
        .unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(cull(1, not_camera, planes(6)))),
        GraphicsError::NotCamera
    );
    world.remove_component(live).unwrap();
    let stale = world.record_offscreen(cull(2, live, planes(6)));
    assert!(matches!(stale, Err(WorldError::StaleComponent(_))));
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_culling_result_holds_at_most_ten_finite_planes() {
    let mut world = world();
    let camera = camera(&mut world);
    assert_eq!(MAX_CULL_PLANES, 10);
    world
        .record_offscreen(cull(1, camera, planes(MAX_CULL_PLANES)))
        .unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(cull(2, camera, planes(MAX_CULL_PLANES + 1)))),
        GraphicsError::InvalidOffscreen
    );
    let mut bad = planes(2);
    bad[1][2] = f32::NAN;
    assert_eq!(
        graphics_error(world.record_offscreen(cull(3, camera, bad))),
        GraphicsError::NotFinite
    );
    let mut infinite = planes(1);
    infinite[0][3] = f32::INFINITY;
    assert_eq!(
        graphics_error(world.record_offscreen(cull(4, camera, infinite))),
        GraphicsError::NotFinite
    );
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn a_handle_is_nonzero_and_issued_once_a_frame() {
    let mut world = world();
    let camera = camera(&mut world);
    assert_eq!(
        graphics_error(world.record_offscreen(cull(0, camera, planes(6)))),
        GraphicsError::InvalidOffscreen
    );
    world.record_offscreen(cull(7, camera, planes(6))).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(cull(7, camera, planes(6)))),
        GraphicsError::InvalidOffscreen
    );
    world.take_offscreen();
    world
        .record_offscreen(cull(7, camera, planes(6)))
        .expect("a taken frame forgets its handles");
}

fn snapshot_of(records: &[OffscreenWork], world: &mut World) -> GraphicsSnapshot {
    for record in records {
        world.record_offscreen(record.clone()).unwrap();
    }
    GraphicsSnapshot::take(world).unwrap()
}

#[test]
fn the_wire_version_is_three_and_every_context_record_round_trips() {
    assert_eq!(WIRE_VERSION, 3);
    let mut world = world();
    let camera = camera(&mut world);
    let records = every_context_record(camera);
    let snapshot = snapshot_of(&records, &mut world);
    assert_eq!(snapshot.work, records);
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn a_culling_result_keeps_the_bits_of_its_planes() {
    let mut world = world();
    let camera = camera(&mut world);
    let odd = vec![[-0.0, 1.0e-38, 0.1, 123_456.79]];
    let record = cull(u64::MAX, camera, odd.clone());
    let bytes = encode_work(&record).unwrap();
    let OffscreenWork::Cull {
        id,
        planes,
        culling_mask,
        options,
        ..
    } = decode_work(&bytes).unwrap()
    else {
        panic!("expected a culling result");
    };
    assert_eq!((id, culling_mask, options), (u64::MAX, 0x8000_0101, 5));
    assert_eq!(planes[0].map(f32::to_bits), odd[0].map(f32::to_bits));
}

#[test]
fn one_context_record_round_trips_without_a_snapshot_around_it() {
    let mut world = world();
    let camera = camera(&mut world);
    for record in every_context_record(camera) {
        let bytes = encode_work(&record).unwrap();
        assert_eq!(decode_work(&bytes).unwrap(), record);
        assert_eq!(
            decode_work(&bytes[..bytes.len() - 1]),
            Err(WireError::Truncated)
        );
        let mut longer = bytes;
        longer.push(0);
        assert_eq!(decode_work(&longer), Err(WireError::TrailingBytes));
    }
}

#[test]
fn a_plane_count_over_the_limit_is_refused_when_decoding() {
    let mut world = world();
    let camera = camera(&mut world);
    let bytes = encode_work(&cull(1, camera, planes(1))).unwrap();
    // The count of planes is the u32 that precedes the plane floats and the options.
    let count_at = bytes.len() - 4 - 16 - 4;
    let mut huge = bytes;
    huge[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode_work(&huge), Err(WireError::TooLong));
}

#[test]
fn context_records_share_the_frame_with_other_work() {
    let mut world = world();
    let camera = camera(&mut world);
    world
        .record_offscreen(OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::CameraTarget],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        })
        .unwrap();
    world.record_offscreen(cull(1, camera, planes(6))).unwrap();
    world
        .record_offscreen(OffscreenWork::ReleaseTemporary { name_id: 3 })
        .unwrap();
    let work = world.take_offscreen();
    assert!(matches!(work[0], OffscreenWork::SetRenderTarget { .. }));
    assert!(matches!(work[1], OffscreenWork::Cull { id: 1, .. }));
    assert!(matches!(work[2], OffscreenWork::ReleaseTemporary { .. }));
}
