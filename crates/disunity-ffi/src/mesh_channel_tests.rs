use super::*;
use crate::meshes::{DisunityMeshInfo, FIRST_PLAYER_MESH, disunity_mesh_store};
use crate::{lock_store, store::create_world};

const FIRST: u64 = FIRST_PLAYER_MESH;
const VERTICES: u32 = 4;

fn new_world() -> u64 {
    create_world(&mut lock_store()).unwrap()
}

/// Stores a four-vertex fixed mesh with no triangles, the pairing every channel store needs.
fn store_fixed(world: u64, identity: u64) -> DisunityResult {
    let info = DisunityMeshInfo {
        identity,
        vertex_count: VERTICES,
        ..Default::default()
    };
    let vertices = [crate::meshes::DisunityMeshVertex::default(); VERTICES as usize];
    unsafe {
        disunity_mesh_store(
            world,
            identity,
            &info,
            vertices.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    }
}

/// Position, tangent, colour and second texture coordinates over four vertices, a triangle list
/// range and a line list range.
struct Sample {
    info: DisunityMeshChannelsInfo,
    channels: Vec<DisunityMeshChannel>,
    data: Vec<f32>,
    indices: Vec<u32>,
    ranges: Vec<DisunityMeshRange>,
}

fn channel(number: u32, dimension: u32, offset: u32) -> DisunityMeshChannel {
    DisunityMeshChannel {
        number,
        dimension,
        offset,
    }
}

fn sample(identity: u64) -> Sample {
    let channels = vec![
        channel(0, 3, 0),
        channel(2, 4, 12),
        channel(3, 4, 28),
        channel(5, 2, 44),
    ];
    let data: Vec<f32> = (0..52).map(|i| i as f32 * 0.5).collect();
    let indices = vec![0, 1, 2, 2, 1, 3, 0, 1, 1, 2];
    let ranges = vec![
        DisunityMeshRange {
            start: 0,
            count: 6,
            base_vertex: 0,
            topology: 0,
        },
        DisunityMeshRange {
            start: 6,
            count: 4,
            base_vertex: 0,
            topology: 2,
        },
    ];
    Sample {
        info: DisunityMeshChannelsInfo {
            identity,
            vertex_count: VERTICES,
            channel_count: 4,
            float_count: 52,
            index_count: 10,
            range_count: 2,
            ..Default::default()
        },
        channels,
        data,
        indices,
        ranges,
    }
}

fn ptr<T>(items: &[T]) -> *const T {
    if items.is_empty() {
        std::ptr::null()
    } else {
        items.as_ptr()
    }
}

fn ptr_mut<T>(items: &mut [T]) -> *mut T {
    if items.is_empty() {
        std::ptr::null_mut()
    } else {
        items.as_mut_ptr()
    }
}

fn store(world: u64, held: &Sample) -> DisunityResult {
    unsafe {
        disunity_mesh_channels_store(
            world,
            &held.info,
            ptr(&held.channels),
            ptr(&held.data),
            ptr(&held.indices),
            ptr(&held.ranges),
        )
    }
}

struct Read {
    result: DisunityResult,
    info: DisunityMeshChannelsInfo,
    channels: Vec<DisunityMeshChannel>,
    data: Vec<f32>,
    indices: Vec<u32>,
    ranges: Vec<DisunityMeshRange>,
}

/// Reads with the given capacities (channels, floats, indices, ranges), buffers pre-filled with a
/// sentinel.
fn read(world: u64, identity: u64, known: u64, capacity: [u32; 4]) -> Read {
    let [
        channel_capacity,
        float_capacity,
        index_capacity,
        range_capacity,
    ] = capacity;
    let mut info = DisunityMeshChannelsInfo {
        identity,
        generation: known,
        channel_count: channel_capacity,
        float_count: float_capacity,
        index_count: index_capacity,
        range_count: range_capacity,
        ..Default::default()
    };
    let mut channels = vec![channel(77, 77, 77); channel_capacity as usize];
    let mut data = vec![-9.0; float_capacity as usize];
    let mut indices = vec![0xEE; index_capacity as usize];
    let mut ranges = vec![
        DisunityMeshRange {
            start: 77,
            ..Default::default()
        };
        range_capacity as usize
    ];
    let result = unsafe {
        disunity_mesh_channels_read(
            world,
            &mut info,
            ptr_mut(&mut channels),
            ptr_mut(&mut data),
            ptr_mut(&mut indices),
            ptr_mut(&mut ranges),
        )
    };
    Read {
        result,
        info,
        channels,
        data,
        indices,
        ranges,
    }
}

const FULL: [u32; 4] = [4, 52, 10, 2];

#[test]
fn stored_channels_read_back_with_every_topology() {
    let world = new_world();
    let held = sample(FIRST + 3);
    assert_eq!(store_fixed(world, FIRST + 3).code, 0);
    assert_eq!(store(world, &held).code, 0);
    let got = read(world, FIRST + 3, 0, FULL);
    assert_eq!(got.result.code, 0);
    assert_eq!(got.channels, held.channels);
    assert_eq!(got.data, held.data);
    assert_eq!(got.indices, held.indices);
    assert_eq!(got.ranges, held.ranges);
    let info = got.info;
    assert_eq!((info.present, info.changed, info.copied), (1, 1, 1));
    assert_eq!((info.identity, info.generation), (FIRST + 3, 1));
    assert_eq!(
        (
            info.vertex_count,
            info.channel_count,
            info.float_count,
            info.index_count,
            info.range_count
        ),
        (4, 4, 52, 10, 2)
    );
}

#[test]
fn the_channel_generation_agrees_with_the_fixed_mesh_it_pairs_with() {
    let world = new_world();
    store_fixed(world, FIRST);
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    assert_eq!(read(world, FIRST, 0, [0; 4]).info.generation, 2);
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    assert_eq!(read(world, FIRST, 0, [0; 4]).info.generation, 3);
}

#[test]
fn storing_the_channels_again_alone_raises_their_generation() {
    let world = new_world();
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    let mut changed = sample(FIRST);
    changed.data[0] = 100.0;
    store(world, &changed);
    let got = read(world, FIRST, 1, FULL);
    assert_eq!((got.info.generation, got.info.changed), (2, 1));
    assert_eq!(got.data[0].to_bits(), 100.0f32.to_bits());
}

#[test]
fn storing_the_fixed_mesh_again_drops_channels_that_no_longer_match_it() {
    let world = new_world();
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    store_fixed(world, FIRST);
    assert_eq!(read(world, FIRST, 0, FULL).info.present, 0);
}

#[test]
fn a_read_of_a_generation_already_held_copies_nothing() {
    let world = new_world();
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    let got = read(world, FIRST, 1, FULL);
    assert_eq!(got.result.code, 0);
    assert_eq!(
        (got.info.present, got.info.changed, got.info.copied),
        (1, 0, 0)
    );
    assert_eq!(got.info.float_count, 52);
    assert_eq!(got.indices, vec![0xEE; 10]);
}

#[test]
fn a_zero_capacity_read_reports_the_sizes_and_copies_nothing() {
    let world = new_world();
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    let got = read(world, FIRST, 0, [0; 4]);
    assert_eq!(got.result.code, 0);
    let info = got.info;
    assert_eq!((info.present, info.changed, info.copied), (1, 1, 0));
    assert_eq!(
        (
            info.channel_count,
            info.float_count,
            info.index_count,
            info.range_count
        ),
        (4, 52, 10, 2)
    );
}

#[test]
fn a_capacity_below_a_size_is_an_error_and_leaves_the_buffers_alone() {
    let world = new_world();
    store_fixed(world, FIRST);
    store(world, &sample(FIRST));
    for capacity in [
        [3, 52, 10, 2],
        [4, 51, 10, 2],
        [4, 52, 9, 2],
        [4, 52, 10, 1],
    ] {
        let got = read(world, FIRST, 0, capacity);
        assert_ne!(got.result.code, 0, "{capacity:?}");
        assert!(got.indices.iter().all(|index| *index == 0xEE));
        assert!(got.channels.iter().all(|c| c.number == 77));
        assert!(got.ranges.iter().all(|r| r.start == 77));
    }
}

#[test]
fn an_identity_with_no_channels_reads_as_not_present() {
    let world = new_world();
    store_fixed(world, FIRST);
    let got = read(world, FIRST, 0, FULL);
    assert_eq!(got.result.code, 0);
    assert_eq!((got.info.present, got.info.identity), (0, FIRST));
    assert_eq!(read(world, FIRST + 9, 0, FULL).info.present, 0);
}

#[test]
fn a_store_for_an_identity_with_no_fixed_mesh_is_rejected() {
    let world = new_world();
    assert_ne!(store(world, &sample(FIRST)).code, 0);
    assert_eq!(read(world, FIRST, 0, [0; 4]).info.present, 0);
}

#[test]
fn a_store_that_disagrees_with_the_fixed_vertex_count_is_rejected() {
    let world = new_world();
    store_fixed(world, FIRST);
    let mut bad = sample(FIRST);
    bad.info.vertex_count = 5;
    assert_ne!(store(world, &bad).code, 0);
}

fn assert_rejected(edit: impl FnOnce(&mut Sample), what: &str) {
    let world = new_world();
    store_fixed(world, FIRST);
    let mut bad = sample(FIRST);
    edit(&mut bad);
    assert_ne!(store(world, &bad).code, 0, "{what}");
    assert_eq!(read(world, FIRST, 0, [0; 4]).info.present, 0, "{what}");
}

#[test]
fn a_channel_that_is_duplicated_or_out_of_range_is_rejected() {
    assert_rejected(|s| s.channels[1].number = 0, "duplicate channel");
    assert_rejected(|s| s.channels[1].number = 14, "channel 14");
}

#[test]
fn a_dimension_outside_one_to_four_is_rejected() {
    assert_rejected(|s| s.channels[3].dimension = 5, "dimension 5");
    assert_rejected(|s| s.channels[3].dimension = 0, "dimension 0");
}

#[test]
fn a_channel_reaching_past_the_data_is_rejected() {
    assert_rejected(|s| s.channels[3].offset = 45, "offset past the end");
    assert_rejected(|s| s.channels[3].offset = u32::MAX, "offset overflow");
}

#[test]
fn a_range_past_the_indices_or_of_an_unknown_topology_is_rejected() {
    assert_rejected(|s| s.ranges[1].count = 5, "range past the indices");
    assert_rejected(|s| s.ranges[1].start = u32::MAX, "range start overflow");
    assert_rejected(|s| s.ranges[1].topology = 5, "topology 5");
}

#[test]
fn an_index_past_the_vertices_is_rejected() {
    assert_rejected(|s| s.indices[9] = 4, "index 4 of 4 vertices");
}

#[test]
fn a_value_that_is_not_finite_is_rejected() {
    assert_rejected(|s| s.data[7] = f32::NAN, "NaN");
    assert_rejected(|s| s.data[51] = f32::INFINITY, "infinity");
}

#[test]
fn a_store_beyond_the_bounds_or_with_a_null_buffer_is_rejected() {
    let world = new_world();
    store_fixed(world, FIRST);
    let held = sample(FIRST);
    let huge = DisunityMeshChannelsInfo {
        float_count: MAX_FLOATS + 1,
        ..held.info
    };
    let result = unsafe {
        disunity_mesh_channels_store(
            world,
            &huge,
            held.channels.as_ptr(),
            held.data.as_ptr(),
            held.indices.as_ptr(),
            held.ranges.as_ptr(),
        )
    };
    assert_ne!(result.code, 0);
    let result = unsafe {
        disunity_mesh_channels_store(
            world,
            &held.info,
            held.channels.as_ptr(),
            std::ptr::null(),
            held.indices.as_ptr(),
            held.ranges.as_ptr(),
        )
    };
    assert_ne!(result.code, 0);
    assert_eq!(read(world, FIRST, 0, [0; 4]).info.present, 0);
}

#[test]
fn a_mesh_with_no_channels_and_no_ranges_is_stored_and_reads_back_empty() {
    let world = new_world();
    store_fixed(world, FIRST);
    let empty = Sample {
        info: DisunityMeshChannelsInfo {
            identity: FIRST,
            vertex_count: VERTICES,
            ..Default::default()
        },
        channels: vec![],
        data: vec![],
        indices: vec![],
        ranges: vec![],
    };
    assert_eq!(store(world, &empty).code, 0);
    let got = read(world, FIRST, 0, [0; 4]);
    assert_eq!((got.info.present, got.info.channel_count), (1, 0));
}

#[test]
fn channels_belong_to_their_world_and_unloading_it_drops_them() {
    let first = new_world();
    let second = new_world();
    store_fixed(first, FIRST);
    store(first, &sample(FIRST));
    assert_eq!(read(second, FIRST, 0, [0; 4]).info.present, 0);
    crate::store::destroy_world(&mut lock_store(), first).unwrap();
    assert_ne!(read(first, FIRST, 0, [0; 4]).result.code, 0);
}
