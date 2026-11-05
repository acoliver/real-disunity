//! The graphics snapshot through the ABI: a size query never drains, a copy hands the records out
//! once, and a buffer that is too small leaves them where they are.
use worldrunner_disunity_ffi::{
    abi::*, compute::*, graphics::*, graphics_offscreen::*, graphics_snapshot::*, scheduling::*,
};
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, OffscreenWork, snapshot_wire};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
        ..Default::default()
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

/// Records a blit with the given pass into `target`.
fn blit(world: u64, target: u64, pass: i32) {
    let record = DisunityOffscreenWork {
        kind: 1,
        target,
        pass,
        ..Default::default()
    };
    unsafe { assert_eq!(disunity_offscreen_record(world, &record).code, 0) };
}

fn query(world: u64) -> u32 {
    let mut length = u32::MAX;
    unsafe {
        let result = disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, &mut length);
        assert_eq!(result.code, 0);
    }
    length
}

fn copy(world: u64, capacity: u32) -> (DisunityResult, Vec<u8>, u32) {
    let mut bytes = vec![0_u8; capacity as usize];
    let mut length = u32::MAX;
    let result =
        unsafe { disunity_graphics_snapshot(world, bytes.as_mut_ptr(), capacity, &mut length) };
    (result, bytes, length)
}

/// Asks for the size, then copies the snapshot out and decodes it.
fn snapshot(world: u64) -> GraphicsSnapshot {
    let length = query(world);
    let (result, bytes, written) = copy(world, length);
    assert_eq!(result.code, 0);
    assert_eq!(written, length);
    snapshot_wire::decode(&bytes).unwrap()
}

fn pending_records(world: u64) -> u32 {
    let mut count = 0;
    unsafe {
        let result = disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count);
        assert_eq!(result.code, 0);
    }
    count
}

fn passes(snapshot: &GraphicsSnapshot) -> Vec<i32> {
    snapshot
        .work
        .iter()
        .map(|record| match record {
            OffscreenWork::Blit { pass, .. } => *pass,
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn a_size_query_then_a_copy_returns_the_records_once() {
    let world = new_world();
    let target = render_texture(world);
    blit(world, target, 1);
    blit(world, target, -1);
    let first = query(world);
    assert_eq!(
        query(world),
        first,
        "asking again does not drain or change the size"
    );
    assert_eq!(
        pending_records(world),
        0,
        "the first query took the records into the session"
    );
    let (result, bytes, written) = copy(world, first);
    assert_eq!((result.code, written), (0, first));
    let taken = snapshot_wire::decode(&bytes).unwrap();
    assert_eq!(passes(&taken), [1, -1]);
    assert_eq!(taken.render_textures.len(), 1);
    assert!(
        snapshot(world).work.is_empty(),
        "a second snapshot has no records"
    );
}

#[test]
fn a_world_without_work_has_an_empty_snapshot() {
    let world = new_world();
    render_texture(world);
    let empty = snapshot(world);
    assert_eq!(empty, GraphicsSnapshot::default());
    assert_eq!(snapshot(world), empty);
}

#[test]
fn a_buffer_that_is_too_small_fails_without_draining() {
    let world = new_world();
    let target = render_texture(world);
    blit(world, target, 3);
    let length = query(world);
    let (result, bytes, written) = copy(world, length - 1);
    assert_ne!(result.code, 0);
    assert_eq!(
        written, length,
        "the failed call still reports the size that is needed"
    );
    assert!(bytes.iter().all(|byte| *byte == 0), "nothing was copied");
    assert_eq!(passes(&snapshot(world)), [3], "the records are still there");
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn work_recorded_after_a_query_comes_with_the_next_snapshot() {
    let world = new_world();
    let target = render_texture(world);
    blit(world, target, 1);
    query(world);
    blit(world, target, 2);
    let first = copy(world, query(world)).1;
    assert_eq!(passes(&snapshot_wire::decode(&first).unwrap()), [1]);
    assert_eq!(
        passes(&snapshot(world)),
        [2],
        "no record is lost or repeated"
    );
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_snapshot_nobody_copied_is_dropped_when_the_world_advances() {
    let world = new_world();
    let target = render_texture(world);
    blit(world, target, 1);
    query(world);
    let mut advance = DisunityAdvance::default();
    unsafe { assert_eq!(disunity_world_advance(world, 0, 1, &mut advance).code, 0) };
    assert!(snapshot(world).work.is_empty());
}

/// A compute shader with one kernel and a buffer bound to it, and a dispatch recorded.
fn dispatch_with_buffer(world: u64) {
    let blob = "Readbackresources.assetsRead";
    let desc = DisunityComputeShaderDesc {
        path_id: 8167,
        name_length: 8,
        file_length: 16,
        kernel_count: 1,
    };
    let kernels = [DisunityComputeKernel {
        name_offset: 24,
        name_length: 4,
        size_x: 8,
        size_y: 8,
        size_z: 1,
    }];
    let buffer_desc = DisunityComputeBufferDesc {
        count: 4,
        stride: 4,
        kind: 0,
    };
    let (mut shader, mut buffer) = (0, 0);
    let name = "_Output";
    unsafe {
        let blob_length = u32::try_from(blob.len()).unwrap();
        let made = disunity_compute_shader_create(
            world,
            &desc,
            blob.as_ptr(),
            blob_length,
            kernels.as_ptr(),
            &mut shader,
        );
        assert_eq!(made.code, 0);
        let made = disunity_compute_buffer_create(world, &buffer_desc, &mut buffer);
        assert_eq!(made.code, 0);
        let name_length = u32::try_from(name.len()).unwrap();
        let bound =
            disunity_compute_buffer_bind(world, shader, 0, name.as_ptr(), name_length, buffer);
        assert_eq!(bound.code, 0);
        assert_eq!(disunity_compute_dispatch(world, shader, 0, 2, 1, 1).code, 0);
    }
}

#[test]
fn the_snapshot_carries_a_dispatch_with_its_shader_and_buffers() {
    let world = new_world();
    dispatch_with_buffer(world);
    let taken = snapshot(world);
    assert_eq!(taken.work.len(), 1);
    assert_eq!(taken.compute_shaders.len(), 1);
    assert_eq!(taken.compute_buffers.len(), 1);
    let buffer = taken.compute_buffers.values().next().unwrap();
    assert_eq!(buffer.contents.len(), 16);
}

#[test]
fn bad_arguments_are_refused_and_drain_nothing() {
    let world = new_world();
    let target = render_texture(world);
    blit(world, target, 1);
    let mut length = 0;
    unsafe {
        assert_ne!(
            disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_graphics_snapshot(world, std::ptr::null_mut(), 8, &mut length).code,
            0
        );
        assert_ne!(
            disunity_graphics_snapshot(world + 1000, std::ptr::null_mut(), 0, &mut length).code,
            0
        );
    }
    assert_eq!(passes(&snapshot(world)), [1]);
}
