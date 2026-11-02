use super::*;
use crate::{lock_store, store::create_world};

const FIRST: u64 = FIRST_PLAYER_MESH;

fn vertex(x: f32, y: f32, z: f32) -> DisunityMeshVertex {
    DisunityMeshVertex {
        x,
        y,
        z,
        ..Default::default()
    }
}

/// A triangle and a second triangle sharing its edge, in two sub-meshes.
struct Sample {
    info: DisunityMeshInfo,
    vertices: Vec<DisunityMeshVertex>,
    indices: Vec<u32>,
    submeshes: Vec<DisunityMeshSubMesh>,
}

fn sample(identity: u64) -> Sample {
    let vertices = vec![
        vertex(0.0, 0.0, 0.0),
        vertex(1.0, 0.0, 0.0),
        vertex(0.0, 1.0, 0.0),
        DisunityMeshVertex {
            nx: 0.0,
            ny: 0.0,
            nz: 1.0,
            u: 0.5,
            v: 0.25,
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.4,
            ..vertex(1.0, 1.0, 0.0)
        },
    ];
    Sample {
        info: DisunityMeshInfo {
            identity,
            channels: CHANNEL_NORMALS | CHANNEL_UV0 | CHANNEL_COLORS,
            vertex_count: 4,
            index_count: 6,
            submesh_count: 2,
            left_out: 1,
            ..Default::default()
        },
        vertices,
        indices: vec![0, 1, 2, 2, 1, 3],
        submeshes: vec![
            DisunityMeshSubMesh {
                slot: 0,
                first_index: 0,
                index_count: 3,
            },
            DisunityMeshSubMesh {
                slot: 2,
                first_index: 3,
                index_count: 3,
            },
        ],
    }
}

fn ptr<T>(items: &[T]) -> *const T {
    if items.is_empty() {
        std::ptr::null()
    } else {
        items.as_ptr()
    }
}

fn store(world: u64, identity: u64, sample: &Sample) -> DisunityResult {
    unsafe {
        disunity_mesh_store(
            world,
            identity,
            &sample.info,
            ptr(&sample.vertices),
            ptr(&sample.indices),
            ptr(&sample.submeshes),
        )
    }
}

struct Read {
    result: DisunityResult,
    info: DisunityMeshInfo,
    vertices: Vec<DisunityMeshVertex>,
    indices: Vec<u32>,
    submeshes: Vec<DisunityMeshSubMesh>,
}

/// Reads with the given capacities, buffers pre-filled with a sentinel.
fn read(world: u64, identity: u64, known: u64, capacity: [u32; 3]) -> Read {
    let [vertex_capacity, index_capacity, submesh_capacity] = capacity;
    let mut info = DisunityMeshInfo {
        generation: known,
        vertex_count: vertex_capacity,
        index_count: index_capacity,
        submesh_count: submesh_capacity,
        ..Default::default()
    };
    let mut vertices = vec![vertex(-9.0, -9.0, -9.0); vertex_capacity as usize];
    let mut indices = vec![0xEE; index_capacity as usize];
    let mut submeshes = vec![
        DisunityMeshSubMesh {
            slot: 77,
            ..Default::default()
        };
        submesh_capacity as usize
    ];
    let result = unsafe {
        disunity_mesh_read(
            world,
            identity,
            &mut info,
            if vertices.is_empty() {
                std::ptr::null_mut()
            } else {
                vertices.as_mut_ptr()
            },
            if indices.is_empty() {
                std::ptr::null_mut()
            } else {
                indices.as_mut_ptr()
            },
            if submeshes.is_empty() {
                std::ptr::null_mut()
            } else {
                submeshes.as_mut_ptr()
            },
        )
    };
    Read {
        result,
        info,
        vertices,
        indices,
        submeshes,
    }
}

fn new_world() -> u64 {
    create_world(&mut lock_store()).unwrap()
}

#[test]
fn a_stored_mesh_reads_back_with_every_channel_and_sub_mesh() {
    let world = new_world();
    let held = sample(FIRST + 3);
    assert_eq!(store(world, FIRST + 3, &held).code, 0);
    let got = read(world, FIRST + 3, 0, [4, 6, 2]);
    assert_eq!(got.result.code, 0);
    assert_eq!(got.vertices, held.vertices);
    assert_eq!(got.indices, held.indices);
    assert_eq!(got.submeshes, held.submeshes);
    let info = got.info;
    assert_eq!((info.present, info.changed, info.copied), (1, 1, 1));
    assert_eq!((info.identity, info.generation), (FIRST + 3, 1));
    assert_eq!(
        (info.vertex_count, info.index_count, info.submesh_count),
        (4, 6, 2)
    );
    assert_eq!(
        (info.channels, info.left_out),
        (CHANNEL_NORMALS | CHANNEL_UV0 | CHANNEL_COLORS, 1)
    );
}

#[test]
fn storing_an_identity_again_raises_its_generation_and_replaces_the_data() {
    let world = new_world();
    store(world, FIRST, &sample(FIRST));
    assert_eq!(read(world, FIRST, 0, [0; 3]).info.generation, 1);
    let mut changed = sample(FIRST);
    changed.vertices[0] = vertex(5.0, 6.0, 7.0);
    store(world, FIRST, &changed);
    store(world, FIRST + 1, &sample(FIRST + 1));
    let got = read(world, FIRST, 1, [4, 6, 2]);
    assert_eq!((got.info.generation, got.info.changed), (2, 1));
    assert_eq!(got.vertices[0], vertex(5.0, 6.0, 7.0));
    assert_eq!(read(world, FIRST + 1, 0, [0; 3]).info.generation, 1);
}

#[test]
fn a_read_of_a_generation_already_held_copies_nothing() {
    let world = new_world();
    store(world, FIRST, &sample(FIRST));
    let got = read(world, FIRST, 1, [4, 6, 2]);
    assert_eq!(got.result.code, 0);
    assert_eq!(
        (got.info.present, got.info.changed, got.info.copied),
        (1, 0, 0)
    );
    assert_eq!(got.info.vertex_count, 4);
    assert_eq!(got.indices, vec![0xEE; 6]);
    assert_eq!(got.vertices, vec![vertex(-9.0, -9.0, -9.0); 4]);
}

#[test]
fn a_zero_capacity_read_reports_the_sizes_and_copies_nothing() {
    let world = new_world();
    store(world, FIRST, &sample(FIRST));
    let got = read(world, FIRST, 0, [0; 3]);
    assert_eq!(got.result.code, 0);
    let info = got.info;
    assert_eq!((info.present, info.changed, info.copied), (1, 1, 0));
    assert_eq!(
        (info.vertex_count, info.index_count, info.submesh_count),
        (4, 6, 2)
    );
}

#[test]
fn a_capacity_below_a_size_is_an_error_and_leaves_the_buffers_alone() {
    let world = new_world();
    store(world, FIRST, &sample(FIRST));
    for capacity in [[3, 6, 2], [4, 5, 2], [4, 6, 1]] {
        let got = read(world, FIRST, 0, capacity);
        assert_ne!(got.result.code, 0, "{capacity:?}");
        assert!(got.indices.iter().all(|index| *index == 0xEE));
        assert!(got.vertices.iter().all(|v| *v == vertex(-9.0, -9.0, -9.0)));
        assert!(got.submeshes.iter().all(|s| s.slot == 77));
    }
}

#[test]
fn an_unknown_identity_reads_as_not_present() {
    let world = new_world();
    let got = read(world, FIRST + 9, 0, [4, 6, 2]);
    assert_eq!(got.result.code, 0);
    assert_eq!((got.info.present, got.info.identity), (0, FIRST + 9));
    assert_eq!(got.indices, vec![0xEE; 6]);
}

#[test]
fn an_identity_below_the_mesh_range_is_rejected() {
    let world = new_world();
    assert_ne!(store(world, FIRST - 1, &sample(FIRST - 1)).code, 0);
    assert_eq!(read(world, FIRST - 1, 0, [0; 3]).info.present, 0);
}

#[test]
fn a_description_naming_another_identity_is_rejected() {
    let world = new_world();
    assert_ne!(store(world, FIRST, &sample(FIRST + 1)).code, 0);
    assert_eq!(read(world, FIRST, 0, [0; 3]).info.present, 0);
}

#[test]
fn an_index_past_the_vertices_is_rejected() {
    let world = new_world();
    let mut bad = sample(FIRST);
    bad.indices[5] = 4;
    assert_ne!(store(world, FIRST, &bad).code, 0);
    assert_eq!(read(world, FIRST, 0, [0; 3]).info.present, 0);
}

#[test]
fn indices_that_do_not_make_whole_triangles_are_rejected() {
    let world = new_world();
    let mut bad = sample(FIRST);
    bad.indices.pop();
    bad.info.index_count = 5;
    bad.submeshes.pop();
    assert_ne!(store(world, FIRST, &bad).code, 0);
}

#[test]
fn a_sub_mesh_outside_the_indices_or_off_a_triangle_is_rejected() {
    let world = new_world();
    for (first, count) in [(3, 6), (6, 3), (1, 3), (0, 4)] {
        let mut bad = sample(FIRST);
        bad.submeshes[1] = DisunityMeshSubMesh {
            slot: 2,
            first_index: first,
            index_count: count,
        };
        assert_ne!(store(world, FIRST, &bad).code, 0, "{first} + {count}");
    }
    let mut overflow = sample(FIRST);
    overflow.submeshes[1].first_index = u32::MAX;
    overflow.submeshes[1].index_count = 3;
    assert_ne!(store(world, FIRST, &overflow).code, 0);
}

#[test]
fn a_value_that_is_not_finite_or_a_channel_that_does_not_exist_is_rejected() {
    let world = new_world();
    let mut infinite = sample(FIRST);
    infinite.vertices[2].y = f32::INFINITY;
    assert_ne!(store(world, FIRST, &infinite).code, 0);
    let mut not_a_number = sample(FIRST);
    not_a_number.vertices[0].u = f32::NAN;
    assert_ne!(store(world, FIRST, &not_a_number).code, 0);
    let mut channel = sample(FIRST);
    channel.info.channels = 8;
    assert_ne!(store(world, FIRST, &channel).code, 0);
}

#[test]
fn a_mesh_beyond_the_bounds_or_with_null_buffers_is_rejected() {
    let world = new_world();
    let huge = DisunityMeshInfo {
        identity: FIRST,
        vertex_count: MAX_VERTICES + 1,
        ..Default::default()
    };
    let result = unsafe {
        disunity_mesh_store(
            world,
            FIRST,
            &huge,
            [vertex(0.0, 0.0, 0.0)].as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_ne!(result.code, 0);
    let held = sample(FIRST);
    let result = unsafe {
        disunity_mesh_store(
            world,
            FIRST,
            &held.info,
            std::ptr::null(),
            held.indices.as_ptr(),
            held.submeshes.as_ptr(),
        )
    };
    assert_ne!(result.code, 0);
    assert_eq!(read(world, FIRST, 0, [0; 3]).info.present, 0);
}

#[test]
fn an_empty_mesh_is_stored_and_reads_back_empty() {
    let world = new_world();
    let empty = Sample {
        info: DisunityMeshInfo {
            identity: FIRST,
            ..Default::default()
        },
        vertices: vec![],
        indices: vec![],
        submeshes: vec![],
    };
    assert_eq!(store(world, FIRST, &empty).code, 0);
    let got = read(world, FIRST, 0, [0; 3]);
    assert_eq!(
        (got.info.present, got.info.vertex_count, got.info.generation),
        (1, 0, 1)
    );
}

#[test]
fn meshes_belong_to_their_world_and_unloading_it_drops_them() {
    let first = new_world();
    let second = new_world();
    store(first, FIRST, &sample(FIRST));
    assert_eq!(read(second, FIRST, 0, [0; 3]).info.present, 0);
    crate::store::destroy_world(&mut lock_store(), first).unwrap();
    assert_ne!(read(first, FIRST, 0, [0; 3]).result.code, 0);
    assert_ne!(store(first, FIRST, &sample(FIRST)).code, 0);
    let reused = new_world();
    assert_eq!(read(reused, FIRST, 0, [0; 3]).info.present, 0);
}
