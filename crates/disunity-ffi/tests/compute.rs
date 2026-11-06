//! Compute shaders, compute buffers, kernel bindings and recorded dispatches through the ABI:
//! validation, and that accepted calls reach the world.
use worldrunner_disunity_ffi::{abi::*, compute::*, with_native_world};
use worldrunner_world_simulation::graphics::ComputeShaderId;

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

fn kernel(name_offset: u32, name_length: u32, size: [u32; 3]) -> DisunityComputeKernel {
    DisunityComputeKernel {
        name_offset,
        name_length,
        size_x: size[0],
        size_y: size[1],
        size_z: size[2],
    }
}

/// "Readback" from "resources.assets" with kernels "Read" (8 by 8 by 1) and "Clear" (unknown).
fn shader(world: u64) -> u64 {
    let blob = "Readbackresources.assetsReadClear";
    let desc = DisunityComputeShaderDesc {
        path_id: 8167,
        name_length: 8,
        file_length: 16,
        kernel_count: 2,
    };
    let kernels = [kernel(24, 4, [8, 8, 1]), kernel(28, 5, [0, 0, 0])];
    let (bytes, length) = text(blob);
    let mut id = 0;
    let result = unsafe {
        disunity_compute_shader_create(world, &desc, bytes, length, kernels.as_ptr(), &mut id)
    };
    assert_eq!(result.code, 0);
    id
}

fn buffer(world: u64, count: u32, stride: u32) -> u64 {
    let desc = DisunityComputeBufferDesc {
        count,
        stride,
        kind: 0,
    };
    let mut id = 0;
    let result = unsafe { disunity_compute_buffer_create(world, &desc, &mut id) };
    assert_eq!(result.code, 0, "{count} {stride}");
    id
}

#[test]
fn a_compute_shader_reads_back_what_it_was_created_with() {
    let world = new_world();
    let id = shader(world);
    let mut info = DisunityComputeShaderInfo::default();
    unsafe {
        let probe = disunity_compute_shader_read(
            world,
            id,
            &mut info,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(probe.code, 0);
    }
    assert_eq!(
        (
            info.path_id,
            info.name_length,
            info.file_length,
            info.kernel_count
        ),
        (8167, 8, 16, 2)
    );
    assert_eq!(info.text_length, 33);
    assert_eq!(info.generation, 1);
    let mut kernels = vec![DisunityComputeKernel::default(); 2];
    let mut bytes = vec![0_u8; info.text_length as usize];
    unsafe {
        let read = disunity_compute_shader_read(
            world,
            id,
            &mut info,
            kernels.as_mut_ptr(),
            bytes.as_mut_ptr(),
        );
        assert_eq!(read.code, 0);
    }
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        "Readbackresources.assetsReadClear"
    );
    assert_eq!(kernels[0], kernel(24, 4, [8, 8, 1]));
    assert_eq!(kernels[1], kernel(28, 5, [0, 0, 0]));
}

#[test]
fn reading_into_buffers_that_are_too_small_is_refused() {
    let world = new_world();
    let id = shader(world);
    let mut info = DisunityComputeShaderInfo {
        kernel_count: 1,
        text_length: 33,
        ..Default::default()
    };
    let mut kernels = [DisunityComputeKernel::default(); 2];
    let mut bytes = [0_u8; 33];
    let result = unsafe {
        disunity_compute_shader_read(
            world,
            id,
            &mut info,
            kernels.as_mut_ptr(),
            bytes.as_mut_ptr(),
        )
    };
    assert_ne!(result.code, 0);
    assert_eq!(kernels[0], DisunityComputeKernel::default());
    info = DisunityComputeShaderInfo {
        kernel_count: 2,
        text_length: 10,
        ..Default::default()
    };
    let result = unsafe {
        disunity_compute_shader_read(
            world,
            id,
            &mut info,
            kernels.as_mut_ptr(),
            bytes.as_mut_ptr(),
        )
    };
    assert_ne!(result.code, 0);
    assert_eq!(bytes, [0; 33]);
}

const BLOB: &str = "ReadbackA.assetsKernel";

fn good() -> DisunityComputeShaderDesc {
    DisunityComputeShaderDesc {
        path_id: 1,
        name_length: 8,
        file_length: 8,
        kernel_count: 1,
    }
}

fn create(
    world: u64,
    desc: &DisunityComputeShaderDesc,
    blob: (*const u8, u32),
    kernels: *const DisunityComputeKernel,
    output: *mut u64,
) -> DisunityResult {
    unsafe { disunity_compute_shader_create(world, desc, blob.0, blob.1, kernels, output) }
}

#[test]
fn compute_shader_creation_refuses_missing_pointers_and_bad_text() {
    let world = new_world();
    let (bytes, length) = text(BLOB);
    let kernels = [kernel(16, 6, [4, 4, 4])];
    let mut id = 0;
    let no_output = create(
        world,
        &good(),
        (bytes, length),
        kernels.as_ptr(),
        std::ptr::null_mut(),
    );
    assert_ne!(no_output.code, 0);
    let no_text = create(
        world,
        &good(),
        (std::ptr::null(), length),
        kernels.as_ptr(),
        &mut id,
    );
    assert_ne!(no_text.code, 0);
    let no_kernels = create(world, &good(), (bytes, length), std::ptr::null(), &mut id);
    assert_ne!(no_kernels.code, 0);
    let bad_text = [
        0xFF_u8, 0xFE, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
    ];
    let non_utf8 = DisunityComputeShaderDesc {
        name_length: 2,
        file_length: 4,
        ..good()
    };
    let result = create(
        world,
        &non_utf8,
        (bad_text.as_ptr(), 12),
        kernels.as_ptr(),
        &mut id,
    );
    assert_ne!(result.code, 0);
    assert_eq!(id, 0);
}

#[test]
fn compute_shader_creation_refuses_bad_lengths_and_kernels() {
    let world = new_world();
    let (bytes, length) = text(BLOB);
    let mut id = 0;
    let past_text = DisunityComputeShaderDesc {
        file_length: 30,
        ..good()
    };
    let too_many = DisunityComputeShaderDesc {
        kernel_count: 100_000,
        ..good()
    };
    let kernels = [kernel(16, 6, [4, 4, 4])];
    assert_ne!(
        create(
            world,
            &past_text,
            (bytes, length),
            kernels.as_ptr(),
            &mut id
        )
        .code,
        0
    );
    assert_ne!(
        create(world, &too_many, (bytes, length), kernels.as_ptr(), &mut id).code,
        0
    );
    for bad in [
        [kernel(20, 6, [4, 4, 4])],
        [kernel(u32::MAX, 6, [4, 4, 4])],
        [kernel(16, 6, [4, 0, 4])],
        [kernel(16, 6, [2048, 1, 1])],
    ] {
        let result = create(world, &good(), (bytes, length), bad.as_ptr(), &mut id);
        assert_ne!(result.code, 0);
    }
    assert_eq!(id, 0);
    with_native_world(world, |native| {
        assert_eq!(native.compute_shaders().count(), 0)
    })
    .unwrap();
}

#[test]
fn a_valid_description_creates_the_shader_in_the_world() {
    let world = new_world();
    let (bytes, length) = text(BLOB);
    let kernels = [kernel(16, 6, [4, 4, 4])];
    let mut id = 0;
    let result = create(world, &good(), (bytes, length), kernels.as_ptr(), &mut id);
    assert_eq!(result.code, 0);
    with_native_world(world, |native| {
        let held = native
            .compute_shader(ComputeShaderId::from_bits(id))
            .unwrap();
        assert_eq!(held.name, "Readback");
        assert_eq!(held.source.file, "A.assets");
        assert_eq!(held.kernels[0].name, "Kernel");
        assert_eq!(held.kernels[0].thread_group_size, Some([4, 4, 4]));
    })
    .unwrap();
}

#[test]
fn a_host_fills_in_a_kernel_size_and_a_destroyed_shader_is_stale() {
    let world = new_world();
    let id = shader(world);
    assert_eq!(
        disunity_compute_kernel_size_write(world, id, 1, 16, 4, 1).code,
        0
    );
    assert_ne!(
        disunity_compute_kernel_size_write(world, id, 1, 0, 4, 1).code,
        0
    );
    assert_ne!(
        disunity_compute_kernel_size_write(world, id, 2, 1, 1, 1).code,
        0
    );
    with_native_world(world, |native| {
        let held = native
            .compute_shader(ComputeShaderId::from_bits(id))
            .unwrap();
        assert_eq!(held.kernels[1].thread_group_size, Some([16, 4, 1]));
    })
    .unwrap();
    assert_eq!(disunity_compute_shader_destroy(world, id).code, 0);
    assert_ne!(disunity_compute_shader_destroy(world, id).code, 0);
    assert_ne!(
        disunity_compute_kernel_size_write(world, id, 1, 1, 1, 1).code,
        0
    );
    assert_ne!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
}

#[test]
fn a_compute_buffer_is_created_written_and_read_in_ranges() {
    let world = new_world();
    let id = buffer(world, 4, 4);
    let mut info = DisunityComputeBufferInfo::default();
    unsafe { assert_eq!(disunity_compute_buffer_info(world, id, &mut info).code, 0) };
    assert_eq!(
        (info.count, info.stride, info.kind, info.generation),
        (4, 4, 0, 1)
    );
    let data = [1_u8, 2, 3, 4];
    unsafe {
        assert_eq!(
            disunity_compute_buffer_write(world, id, 8, data.as_ptr(), 4).code,
            0
        );
    }
    let mut out = [9_u8; 16];
    unsafe {
        assert_eq!(
            disunity_compute_buffer_read(world, id, 0, out.as_mut_ptr(), 16).code,
            0
        )
    };
    assert_eq!(out, [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0]);
    let mut tail = [9_u8; 4];
    unsafe {
        assert_eq!(
            disunity_compute_buffer_read(world, id, 8, tail.as_mut_ptr(), 4).code,
            0
        )
    };
    assert_eq!(tail, data);
    unsafe { assert_eq!(disunity_compute_buffer_info(world, id, &mut info).code, 0) };
    assert_eq!(info.generation, 2);
}

#[test]
fn compute_buffer_ranges_and_pointers_are_checked() {
    let world = new_world();
    let id = buffer(world, 2, 4);
    let mut out = [7_u8; 16];
    unsafe {
        let refused = [
            disunity_compute_buffer_write(world, id, 4, out.as_ptr(), 8),
            disunity_compute_buffer_write(world, id, u64::MAX, out.as_ptr(), 2),
            disunity_compute_buffer_write(world, id, 0, std::ptr::null(), 4),
            disunity_compute_buffer_read(world, id, 0, out.as_mut_ptr(), 9),
            disunity_compute_buffer_read(world, id, 5, out.as_mut_ptr(), 4),
            disunity_compute_buffer_read(world, id, 0, std::ptr::null_mut(), 4),
        ];
        assert!(refused.iter().all(|result| result.code != 0));
        assert_eq!(
            disunity_compute_buffer_write(world, id, 0, std::ptr::null(), 0).code,
            0
        );
        assert_eq!(
            disunity_compute_buffer_read(world, id, 8, std::ptr::null_mut(), 0).code,
            0
        );
    }
    assert_eq!(out, [7; 16]);
}

#[test]
fn compute_buffer_shapes_are_limited_and_a_destroyed_buffer_is_stale() {
    let world = new_world();
    let id = buffer(world, 2, 4);
    for (count, stride) in [(0, 4), (1, 0), (1, 6), (1, 2052), (u32::MAX, 2048)] {
        let desc = DisunityComputeBufferDesc {
            count,
            stride,
            kind: 0,
        };
        let mut made = 0;
        let result = unsafe { disunity_compute_buffer_create(world, &desc, &mut made) };
        assert_ne!(result.code, 0, "{count} {stride}");
        assert_eq!(made, 0);
    }
    unsafe {
        let none = disunity_compute_buffer_create(world, std::ptr::null(), std::ptr::null_mut());
        assert_ne!(none.code, 0);
    }
    assert_eq!(disunity_compute_buffer_destroy(world, id).code, 0);
    assert_ne!(disunity_compute_buffer_destroy(world, id).code, 0);
    let mut out = [0_u8; 4];
    let read = unsafe { disunity_compute_buffer_read(world, id, 0, out.as_mut_ptr(), 4) };
    assert_ne!(read.code, 0);
}
