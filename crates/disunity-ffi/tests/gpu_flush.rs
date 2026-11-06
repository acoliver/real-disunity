//! The synchronous GPU flush: the host's callback runs with no world lock held, so it can call
//! the ABI on the same world while the flush is in progress.
use std::sync::mpsc;
use std::time::Duration;

use worldrunner_disunity_ffi::{
    abi::*, buffers::*, compute::*, gpu_flush::*, graphics_offscreen::*,
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn buffer(world: u64, count: u32) -> u64 {
    let desc = DisunityComputeBufferDesc {
        count,
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

/// One compute shader with one kernel whose size is unknown.
fn shader(world: u64) -> u64 {
    let blob = "ShaderA.assetsMain";
    let desc = DisunityComputeShaderDesc {
        path_id: 1,
        name_length: 6,
        file_length: 8,
        kernel_count: 1,
    };
    let kernels = [DisunityComputeKernel {
        name_offset: 14,
        name_length: 4,
        ..Default::default()
    }];
    let mut id = 0;
    let result = unsafe {
        disunity_compute_shader_create(
            world,
            &desc,
            blob.as_ptr(),
            u32::try_from(blob.len()).unwrap(),
            kernels.as_ptr(),
            &mut id,
        )
    };
    assert_eq!(result.code, 0);
    id
}

fn dispatch(world: u64, shader: u64, buffer: u64) {
    let name = "_Output";
    unsafe {
        let bind = disunity_compute_buffer_bind(
            world,
            shader,
            0,
            name.as_ptr(),
            u32::try_from(name.len()).unwrap(),
            buffer,
        );
        assert_eq!(bind.code, 0);
        assert_eq!(disunity_compute_dispatch(world, shader, 0, 3, 1, 1).code, 0);
    }
}

fn read_buffer(world: u64, id: u64, length: u32) -> Vec<u8> {
    let mut bytes = vec![0_u8; length as usize];
    unsafe {
        assert_eq!(
            disunity_compute_buffer_read(world, id, 0, bytes.as_mut_ptr(), length).code,
            0
        );
    }
    bytes
}

fn last_error(world: u64) -> String {
    unsafe {
        let mut owned = DisunityBuffer::default();
        assert_eq!(disunity_error_take(world, &mut owned).code, 0);
        let mut bytes = vec![0_u8; owned.length as usize];
        let mut written = 0;
        let copy =
            disunity_buffer_copy(owned.handle, bytes.as_mut_ptr(), owned.length, &mut written);
        assert_eq!(copy.code, 0);
        assert_eq!(disunity_buffer_free(owned.handle).code, 0);
        String::from_utf8(bytes).unwrap()
    }
}

/// What the callbacks below record and the flush they run in, borrowed through the `user` word.
#[derive(Default)]
struct Probe {
    calls: u32,
    world: u64,
    buffer: u64,
    taken: Vec<u32>,
    nested: Option<i32>,
    fail_with: Option<&'static str>,
}

fn probe<'a>(user: u64) -> &'a mut Probe {
    unsafe { &mut *(user as *mut Probe) }
}

fn write_message(text: &str, message: *mut u8, capacity: u32, length: *mut u32) {
    let bytes = text.as_bytes();
    let used = bytes.len().min(capacity as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), message, used);
        *length = u32::try_from(used).unwrap();
    }
}

/// Plays the host executor: takes the pending work, reads the dispatch's bindings, grows the
/// kernel size and fills the bound buffer with the dispatch's group count, all through the ABI
/// on the world being flushed.
unsafe extern "C" fn executor(
    world: u64,
    user: u64,
    message: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> DisunityResult {
    let state = probe(user);
    state.calls += 1;
    state.world = world;
    if let Some(text) = state.fail_with {
        write_message(text, message, capacity, length);
        return DisunityResult {
            code: result_code::ERROR,
            detail: 0,
        };
    }
    state.nested = Some(disunity_gpu_flush(world).code);
    let mut count = 0;
    let mut records = vec![DisunityOffscreenWork::default(); 8];
    let taken = unsafe { disunity_offscreen_take(world, records.as_mut_ptr(), 8, &mut count) };
    if taken.code != 0 {
        return taken;
    }
    for record in &records[..count as usize] {
        state.taken.push(record.kind);
        if record.kind != 3 {
            continue;
        }
        let size = disunity_compute_kernel_size_write(world, record.shader, record.kernel, 8, 8, 1);
        if size.code != 0 {
            return size;
        }
        let value = (record.kernel + record.groups_x).to_le_bytes();
        let written =
            unsafe { disunity_compute_buffer_write(world, state.buffer, 4, value.as_ptr(), 4) };
        if written.code != 0 {
            return written;
        }
    }
    DisunityResult {
        code: result_code::OK,
        detail: 0,
    }
}

fn register(world: u64, state: &mut Probe) {
    let result = unsafe {
        disunity_gpu_flush_register(world, Some(executor), std::ptr::from_mut(state) as u64)
    };
    assert_eq!(result.code, 0);
}

/// Runs the flush on another thread so that a lock held across the callback fails the test with
/// a timeout instead of hanging it.
fn flush_with_timeout(world: u64) -> DisunityResult {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        sender.send(disunity_gpu_flush(world)).ok();
    });
    receiver
        .recv_timeout(Duration::from_secs(20))
        .expect("the flush callback ran without the world lock held")
}

#[test]
fn without_a_registered_callback_the_flush_does_nothing_and_reads_see_the_cpu_contents() {
    let world = new_world();
    let id = buffer(world, 2);
    let data = [5_u8, 6, 7, 8];
    unsafe {
        assert_eq!(
            disunity_compute_buffer_write(world, id, 0, data.as_ptr(), 4).code,
            0
        )
    };
    assert_eq!(disunity_gpu_flush(world).code, 0);
    assert_eq!(read_buffer(world, id, 8), [5, 6, 7, 8, 0, 0, 0, 0]);
}

#[test]
fn the_flush_of_a_stale_world_is_refused() {
    let world = new_world();
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
    assert_ne!(disunity_gpu_flush(world).code, 0);
    let result = unsafe { disunity_gpu_flush_register(world, Some(executor), 0) };
    assert_ne!(result.code, 0);
}

#[test]
fn the_callback_runs_with_no_lock_held_and_reaches_the_world_through_the_abi() {
    let world = new_world();
    let id = buffer(world, 2);
    let compute = shader(world);
    dispatch(world, compute, id);
    let mut state = Probe {
        buffer: id,
        ..Default::default()
    };
    register(world, &mut state);

    assert_eq!(flush_with_timeout(world).code, 0);

    assert_eq!(state.calls, 1);
    assert_eq!(state.world, world);
    assert_eq!(state.taken, [3]);
    assert_eq!(read_buffer(world, id, 8), [0, 0, 0, 0, 3, 0, 0, 0]);
    let mut info = DisunityComputeShaderInfo {
        kernel_count: 1,
        text_length: 32,
        ..Default::default()
    };
    let mut kernels = [DisunityComputeKernel::default(); 1];
    let mut text = [0_u8; 32];
    unsafe {
        let read = disunity_compute_shader_read(
            world,
            compute,
            &mut info,
            kernels.as_mut_ptr(),
            text.as_mut_ptr(),
        );
        assert_eq!(read.code, 0);
    }
    assert_eq!(
        (kernels[0].size_x, kernels[0].size_y, kernels[0].size_z),
        (8, 8, 1)
    );
}

#[test]
fn a_flush_started_inside_the_flush_is_refused_and_the_outer_flush_still_completes() {
    let world = new_world();
    let id = buffer(world, 2);
    let compute = shader(world);
    dispatch(world, compute, id);
    let mut state = Probe {
        buffer: id,
        ..Default::default()
    };
    register(world, &mut state);
    assert_eq!(flush_with_timeout(world).code, 0);
    assert_eq!(state.calls, 1);
    assert_ne!(state.nested, Some(0));
    assert!(state.nested.is_some());
}

#[test]
fn the_work_is_taken_once_so_a_second_flush_finds_none() {
    let world = new_world();
    let id = buffer(world, 2);
    let compute = shader(world);
    dispatch(world, compute, id);
    let mut state = Probe {
        buffer: id,
        ..Default::default()
    };
    register(world, &mut state);
    assert_eq!(flush_with_timeout(world).code, 0);
    assert_eq!(flush_with_timeout(world).code, 0);
    assert_eq!(state.calls, 2);
    assert_eq!(state.taken, [3]);
}

#[test]
fn a_failing_callback_fails_the_flush_with_its_message() {
    let world = new_world();
    let mut state = Probe {
        fail_with: Some("the executor lost the device"),
        ..Default::default()
    };
    register(world, &mut state);
    let result = flush_with_timeout(world);
    assert_ne!(result.code, 0);
    let message = last_error(world);
    assert!(message.contains("GPU flush"), "{message}");
    assert!(
        message.contains("the executor lost the device"),
        "{message}"
    );
    state.fail_with = None;
    assert_eq!(flush_with_timeout(world).code, 0);
}

#[test]
fn registering_nothing_removes_the_callback() {
    let world = new_world();
    let mut state = Probe::default();
    register(world, &mut state);
    unsafe { assert_eq!(disunity_gpu_flush_register(world, None, 0).code, 0) };
    assert_eq!(flush_with_timeout(world).code, 0);
    assert_eq!(state.calls, 0);
}

#[test]
fn a_callback_belongs_to_the_world_it_was_registered_on() {
    let first = new_world();
    let second = new_world();
    let mut state = Probe::default();
    register(first, &mut state);
    assert_eq!(flush_with_timeout(second).code, 0);
    assert_eq!(state.calls, 0);
    assert_eq!(flush_with_timeout(first).code, 0);
    assert_eq!((state.calls, state.world), (1, first));
}
