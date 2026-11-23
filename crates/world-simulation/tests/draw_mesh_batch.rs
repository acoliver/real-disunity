//! A command buffer's batch draw: the instances a shader reads from a raw buffer. The record is
//! refused unless the mesh identity, the material and the buffer are live, the window lies inside
//! the buffer and the names and lists are within their limits. A refused record changes nothing.
//! An accepted one keeps its place among the frame's records, is carried by the wire unchanged
//! and brings its buffer's contents into the frame's snapshot.
use worldrunner_world_simulation::graphics::snapshot_wire::{
    decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, DrawBatch, GraphicsError, GraphicsSnapshot, MAX_BATCH_PASS_TAGS,
    MAX_BATCH_VISIBLE, MAX_NAME_BYTES, MAX_PROPERTIES_PER_KIND, MaterialId, OffscreenWork,
    WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

const MESH: u64 = 1 << 41;

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Trees".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

/// A buffer of 64 distinct bytes.
fn buffer(world: &mut World) -> ComputeBufferId {
    let id = world.create_compute_buffer(16, 4, 0).unwrap();
    let bytes: Vec<u8> = (0..64).collect();
    world.set_compute_buffer_data(id, 0, &bytes).unwrap();
    id
}

fn batch(buffer: ComputeBufferId) -> DrawBatch {
    DrawBatch {
        buffer,
        offset: 16,
        size: 48,
        metadata: vec![
            ("unity_ObjectToWorld".into(), 0x8000_0010),
            ("_Color".into(), 0x0000_0040),
        ],
        visible: vec![2, 0, 1],
        pass_tags: vec!["UniversalForward".into(), "SRPDefaultUnlit".into()],
    }
}

fn record(material: MaterialId, batch: DrawBatch) -> OffscreenWork {
    OffscreenWork::DrawMeshBatch {
        mesh: MESH,
        material,
        submesh: 1,
        pass: 2,
        batch,
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn a_recorded_batch_reaches_the_snapshot_unchanged_with_its_buffer() {
    let mut world = world();
    let material = material(&mut world);
    let buffer = buffer(&mut world);
    let first = record(material, batch(buffer));
    let empty = record(
        material,
        DrawBatch {
            visible: Vec::new(),
            ..batch(buffer)
        },
    );
    for work in [first.clone(), empty.clone()] {
        world.record_offscreen(work).unwrap();
    }

    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();

    assert_eq!(snapshot.work, vec![first, empty]);
    assert_eq!(snapshot.compute_buffers[&buffer].contents.len(), 64);
    assert_eq!(
        snapshot.materials.len(),
        1,
        "the material is copied for the host"
    );
    assert_eq!(decode(&encode(&snapshot).unwrap()).unwrap(), snapshot);
}

#[test]
fn a_batch_record_round_trips_through_the_wire_under_its_own_tag() {
    let mut world = world();
    let material = material(&mut world);
    let work = record(material, batch(buffer(&mut world)));

    let bytes = encode_work(&work).unwrap();

    assert_eq!(bytes[0], 23);
    assert_eq!(decode_work(&bytes).unwrap(), work);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn a_batch_that_names_nothing_alive_or_lies_outside_its_buffer_is_refused_by_name() {
    let mut world = world();
    let material = material(&mut world);
    let held = buffer(&mut world);
    let gone = buffer(&mut world);
    world.destroy_compute_buffer(gone).unwrap();
    let window = |offset, size| DrawBatch {
        offset,
        size,
        ..batch(held)
    };
    let cases = [
        (
            record(material, batch(gone)),
            GraphicsError::StaleComputeBuffer,
        ),
        (
            record(material, window(32, 48)),
            GraphicsError::ComputeRange,
        ),
        (
            record(material, window(u64::MAX, 2)),
            GraphicsError::ComputeRange,
        ),
        (record(material, window(0, 0)), GraphicsError::ComputeRange),
        (
            record(MaterialId::from_bits(0x0000_0001_0000_0099), batch(held)),
            GraphicsError::StaleMaterial,
        ),
    ];
    for (work, expected) in cases {
        assert_eq!(graphics_error(world.record_offscreen(work)), expected);
    }
    let zero_mesh = OffscreenWork::DrawMeshBatch {
        mesh: 0,
        material,
        submesh: 0,
        pass: -1,
        batch: batch(held),
    };
    assert_eq!(
        graphics_error(world.record_offscreen(zero_mesh)),
        GraphicsError::InvalidOffscreen
    );
    let low_pass = OffscreenWork::DrawMeshBatch {
        mesh: MESH,
        material,
        submesh: 0,
        pass: -2,
        batch: batch(held),
    };
    assert_eq!(
        graphics_error(world.record_offscreen(low_pass)),
        GraphicsError::InvalidOffscreen
    );
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn metadata_and_visible_lists_over_their_limits_are_refused_by_name() {
    let mut world = world();
    let material = material(&mut world);
    let held = buffer(&mut world);
    let long_name = "n".repeat(MAX_NAME_BYTES + 1);
    let with = |metadata: Vec<(String, u32)>, visible: Vec<u32>| {
        record(
            material,
            DrawBatch {
                metadata,
                visible,
                ..batch(held)
            },
        )
    };
    let too_many = vec![("_Color".to_owned(), 0); MAX_PROPERTIES_PER_KIND + 1];
    let cases = [
        (
            with(vec![(long_name, 0)], vec![0]),
            GraphicsError::InvalidName,
        ),
        (
            with(vec![(String::new(), 0)], vec![0]),
            GraphicsError::InvalidName,
        ),
        (with(too_many, vec![0]), GraphicsError::InvalidOffscreen),
        (
            with(Vec::new(), vec![0; MAX_BATCH_VISIBLE + 1]),
            GraphicsError::InvalidOffscreen,
        ),
    ];
    for (work, expected) in cases {
        assert_eq!(graphics_error(world.record_offscreen(work)), expected);
    }
    assert!(world.offscreen_work().is_empty());
    world
        .record_offscreen(with(Vec::new(), vec![0; MAX_BATCH_VISIBLE]))
        .unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn pass_tags_over_their_limits_are_refused_by_name() {
    let mut world = world();
    let material = material(&mut world);
    let held = buffer(&mut world);
    let with = |pass_tags: Vec<String>| {
        record(
            material,
            DrawBatch {
                pass_tags,
                ..batch(held)
            },
        )
    };
    let cases = [
        (
            with(vec!["t".repeat(MAX_NAME_BYTES + 1)]),
            GraphicsError::InvalidName,
        ),
        (with(vec![String::new()]), GraphicsError::InvalidName),
        (
            with(vec!["Tag".to_owned(); MAX_BATCH_PASS_TAGS + 1]),
            GraphicsError::InvalidOffscreen,
        ),
    ];
    for (work, expected) in cases {
        assert_eq!(graphics_error(world.record_offscreen(work)), expected);
    }
    assert!(world.offscreen_work().is_empty());
    world
        .record_offscreen(with(vec!["Tag".to_owned(); MAX_BATCH_PASS_TAGS]))
        .unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn a_buffer_destroyed_after_the_record_is_left_out_of_the_snapshot_and_the_record_stays() {
    let mut world = world();
    let material = material(&mut world);
    let buffer = buffer(&mut world);
    let work = record(material, batch(buffer));
    world.record_offscreen(work.clone()).unwrap();
    world.destroy_compute_buffer(buffer).unwrap();

    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();

    assert_eq!(snapshot.work, vec![work]);
    assert!(snapshot.compute_buffers.is_empty());
}
