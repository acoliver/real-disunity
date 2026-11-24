//! A command buffer's batch draw through the ABI: one call forwards the stored mesh's identity,
//! the material, the sub-mesh, the pass, the buffer window, the metadata names with their words
//! and the visible indexes as one record. Pointers, names and counts that do not agree and
//! everything the world or the mesh store refuses are refused by the call and record nothing.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{abi::*, compute::*, graphics::*, graphics_batch::*, meshes::*};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, DrawBatch, MaterialId, OffscreenWork,
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn material(world: u64) -> u64 {
    let (mut shader, mut material) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_shader_create(world, b"Trees".as_ptr(), 5, &mut shader).code,
            0
        );
        assert_eq!(
            disunity_material_create(world, shader, &mut material).code,
            0
        );
    }
    material
}

fn store_mesh(world: u64, identity: u64) {
    let info = DisunityMeshInfo {
        identity,
        vertex_count: 3,
        index_count: 3,
        submesh_count: 1,
        ..Default::default()
    };
    let vertices = [DisunityMeshVertex::default(); 3];
    let indices = [0_u32, 1, 2];
    let submeshes = [DisunityMeshSubMesh {
        slot: 0,
        first_index: 0,
        index_count: 3,
    }];
    unsafe {
        let stored = disunity_mesh_store(
            world,
            identity,
            &info,
            vertices.as_ptr(),
            indices.as_ptr(),
            submeshes.as_ptr(),
        );
        assert_eq!(stored.code, 0);
    }
}

/// A buffer of 64 bytes.
fn buffer(world: u64) -> u64 {
    let desc = DisunityComputeBufferDesc {
        count: 16,
        stride: 4,
        kind: 0,
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_compute_buffer_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

/// The names and words of the two properties of the tests, and three visible indexes.
const NAMES: &[u8] = b"unity_ObjectToWorld\0_Color";
const WORDS: [u32; 2] = [0x8000_0010, 0x40];
const VISIBLE: [u32; 3] = [2, 0, 1];
const TAGS: &[u8] = b"UniversalForward\0SRPDefaultUnlit";

fn description(buffer: u64, material: u64) -> DisunityDrawBatch {
    DisunityDrawBatch {
        mesh: FIRST_PLAYER_MESH,
        material,
        buffer,
        offset: 16,
        size: 48,
        submesh: 1,
        pass: -1,
        metadata_count: 2,
        names_len: NAMES.len() as u32,
        visible_count: 3,
        tags_len: TAGS.len() as u32,
    }
}

fn call(desc: &DisunityDrawBatch, world: u64) -> i32 {
    call_lists(world, desc, &lists())
}

/// The four list pointers of a call.
struct Lists {
    names: *const u8,
    words: *const u32,
    visible: *const u32,
    tags: *const u8,
}

/// The lists that match `description`.
fn lists() -> Lists {
    Lists {
        names: NAMES.as_ptr(),
        words: WORDS.as_ptr(),
        visible: VISIBLE.as_ptr(),
        tags: TAGS.as_ptr(),
    }
}

/// The call with the record pointer and the list pointers it is given, which a test may make null or wrong.
fn call_lists(world: u64, desc: *const DisunityDrawBatch, lists: &Lists) -> i32 {
    unsafe {
        disunity_offscreen_draw_mesh_batch(
            world,
            desc,
            lists.names,
            lists.words,
            lists.visible,
            lists.tags,
        )
    }
    .code
}

#[test]
fn every_field_reaches_the_frame_as_one_batch_draw() {
    let world = new_world();
    let material = material(world);
    let buffer = buffer(world);
    store_mesh(world, FIRST_PLAYER_MESH);

    assert_eq!(call(&description(buffer, material), world), 0);

    let taken = snapshot(world).work;
    assert_eq!(taken.len(), 1);
    assert_eq!(
        taken[0],
        OffscreenWork::DrawMeshBatch {
            mesh: FIRST_PLAYER_MESH,
            material: MaterialId::from_bits(material),
            submesh: 1,
            pass: -1,
            batch: DrawBatch {
                buffer: ComputeBufferId::from_bits(buffer),
                offset: 16,
                size: 48,
                metadata: vec![
                    ("unity_ObjectToWorld".into(), 0x8000_0010),
                    ("_Color".into(), 0x40),
                ],
                visible: vec![2, 0, 1],
                pass_tags: vec!["UniversalForward".into(), "SRPDefaultUnlit".into()],
            },
        }
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_batch_with_no_metadata_and_no_visible_instance_is_recorded() {
    let world = new_world();
    let (material, buffer) = (material(world), buffer(world));
    store_mesh(world, FIRST_PLAYER_MESH);
    let desc = DisunityDrawBatch {
        metadata_count: 0,
        names_len: 0,
        visible_count: 0,
        tags_len: 0,
        ..description(buffer, material)
    };
    let code = unsafe {
        disunity_offscreen_draw_mesh_batch(
            world,
            &desc,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    }
    .code;
    assert_eq!(code, 0);
    let taken = snapshot(world).work;
    let OffscreenWork::DrawMeshBatch { batch, .. } = &taken[0] else {
        panic!("expected a batch draw, got {:?}", taken[0]);
    };
    assert!(batch.metadata.is_empty() && batch.visible.is_empty() && batch.pass_tags.is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_call_the_pointers_the_names_or_the_world_refuse_records_nothing() {
    let world = new_world();
    let (material, buffer) = (material(world), buffer(world));
    store_mesh(world, FIRST_PLAYER_MESH);
    let good = description(buffer, material);
    let with = |change: &dyn Fn(&mut DisunityDrawBatch)| {
        let mut desc = good;
        change(&mut desc);
        call(&desc, world)
    };
    let bad_utf8: &[u8] = b"\xff\0_Color";
    let refused = [
        with(&|d| d.mesh += 1),
        with(&|d| d.mesh = 0),
        with(&|d| d.material += 1_000),
        with(&|d| d.buffer += 1_000),
        with(&|d| d.offset = 32),
        with(&|d| d.size = 0),
        with(&|d| d.pass = -2),
        with(&|d| d.pass = 2),
        with(&|d| d.tags_len = 17),
        with(&|d| d.metadata_count = 3),
        with(&|d| d.names_len = 3),
        call(&good, world + 999),
        call_lists(world, std::ptr::null(), &lists()),
        call_lists(
            world,
            &good,
            &Lists {
                names: std::ptr::null(),
                ..lists()
            },
        ),
        call_lists(
            world,
            &good,
            &Lists {
                words: std::ptr::null(),
                ..lists()
            },
        ),
        call_lists(
            world,
            &good,
            &Lists {
                visible: std::ptr::null(),
                ..lists()
            },
        ),
        call_lists(
            world,
            &good,
            &Lists {
                tags: std::ptr::null(),
                ..lists()
            },
        ),
        call_lists(
            world,
            &good,
            &Lists {
                tags: bad_utf8.as_ptr(),
                ..lists()
            },
        ),
        call_lists(
            world,
            &good,
            &Lists {
                names: bad_utf8.as_ptr(),
                ..lists()
            },
        ),
    ];
    for (index, code) in refused.iter().enumerate() {
        assert_ne!(*code, 0, "case {index}");
    }
    assert!(snapshot(world).work.is_empty());
    assert_eq!(call(&good, world), 0, "the good call still records");
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
