//! Compute shaders, compute buffers, kernel bindings and recorded dispatches through the ABI:
//! validation, and that accepted calls reach the world.
use worldrunner_disunity_ffi::{
    abi::*, compute::*, compute_bindings::*, graphics::*, graphics_offscreen::*,
    graphics_properties::*, with_native_world,
};
use worldrunner_world_simulation::graphics::{ComputeBufferId, ComputeShaderId, TextureRef};

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

fn work(kind: u32) -> DisunityOffscreenWork {
    DisunityOffscreenWork {
        kind,
        ..Default::default()
    }
}

fn take(world: u64) -> Vec<DisunityOffscreenWork> {
    let mut count = 0;
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count).code,
            0
        );
    }
    let mut records = vec![DisunityOffscreenWork::default(); count as usize];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), count, &mut count).code,
            0
        );
    }
    records
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

#[test]
fn compute_shader_creation_is_validated_before_the_world_sees_it() {
    let world = new_world();
    let blob = "ReadbackA.assetsKernel";
    let (bytes, length) = text(blob);
    let good = DisunityComputeShaderDesc {
        path_id: 1,
        name_length: 8,
        file_length: 8,
        kernel_count: 1,
    };
    let kernels = [kernel(16, 6, [4, 4, 4])];
    let mut id = 0;
    let create = |desc: &DisunityComputeShaderDesc,
                  blob: (*const u8, u32),
                  kernels: *const DisunityComputeKernel,
                  output: *mut u64| unsafe {
        disunity_compute_shader_create(world, desc, blob.0, blob.1, kernels, output)
    };
    assert_ne!(
        create(
            &good,
            (bytes, length),
            kernels.as_ptr(),
            std::ptr::null_mut()
        )
        .code,
        0
    );
    assert_ne!(
        create(&good, (std::ptr::null(), length), kernels.as_ptr(), &mut id).code,
        0
    );
    assert_ne!(
        create(&good, (bytes, length), std::ptr::null(), &mut id).code,
        0
    );
    let bad_text = [
        0xFF_u8, 0xFE, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
    ];
    let non_utf8 = DisunityComputeShaderDesc {
        name_length: 2,
        file_length: 4,
        kernel_count: 1,
        ..good
    };
    assert_ne!(
        create(
            &non_utf8,
            (bad_text.as_ptr(), 12),
            kernels.as_ptr(),
            &mut id
        )
        .code,
        0
    );
    let lengths_past_text = DisunityComputeShaderDesc {
        file_length: 30,
        ..good
    };
    assert_ne!(
        create(
            &lengths_past_text,
            (bytes, length),
            kernels.as_ptr(),
            &mut id
        )
        .code,
        0
    );
    let kernel_past_text = [kernel(20, 6, [4, 4, 4])];
    assert_ne!(
        create(&good, (bytes, length), kernel_past_text.as_ptr(), &mut id).code,
        0
    );
    let overflow = [kernel(u32::MAX, 6, [4, 4, 4])];
    assert_ne!(
        create(&good, (bytes, length), overflow.as_ptr(), &mut id).code,
        0
    );
    let partly_known = [kernel(16, 6, [4, 0, 4])];
    assert_ne!(
        create(&good, (bytes, length), partly_known.as_ptr(), &mut id).code,
        0
    );
    let too_big = [kernel(16, 6, [2048, 1, 1])];
    assert_ne!(
        create(&good, (bytes, length), too_big.as_ptr(), &mut id).code,
        0
    );
    let too_many = DisunityComputeShaderDesc {
        kernel_count: 100_000,
        ..good
    };
    assert_ne!(
        create(&too_many, (bytes, length), kernels.as_ptr(), &mut id).code,
        0
    );
    assert_eq!(id, 0);
    with_native_world(world, |native| {
        assert_eq!(native.compute_shaders().count(), 0)
    })
    .unwrap();
    assert_eq!(
        create(&good, (bytes, length), kernels.as_ptr(), &mut id).code,
        0
    );
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
fn compute_buffer_ranges_limits_and_pointers_are_checked() {
    let world = new_world();
    let id = buffer(world, 2, 4);
    let mut out = [7_u8; 16];
    unsafe {
        assert_ne!(
            disunity_compute_buffer_write(world, id, 4, out.as_ptr(), 8).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_write(world, id, u64::MAX, out.as_ptr(), 2).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_write(world, id, 0, std::ptr::null(), 4).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_read(world, id, 0, out.as_mut_ptr(), 9).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_read(world, id, 5, out.as_mut_ptr(), 4).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_read(world, id, 0, std::ptr::null_mut(), 4).code,
            0
        );
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
        assert_ne!(
            disunity_compute_buffer_create(world, std::ptr::null(), std::ptr::null_mut()).code,
            0
        );
    }
    assert_eq!(disunity_compute_buffer_destroy(world, id).code, 0);
    assert_ne!(disunity_compute_buffer_destroy(world, id).code, 0);
    unsafe {
        assert_ne!(
            disunity_compute_buffer_read(world, id, 0, out.as_mut_ptr(), 4).code,
            0
        );
    }
}

fn int_write(world: u64, shader: u64, kernel: i32, name: &str, value: i32) -> DisunityResult {
    let (bytes, length) = text(name);
    unsafe { disunity_compute_int_write(world, shader, kernel, bytes, length, value) }
}

#[test]
fn values_and_buffers_bound_to_a_kernel_arrive_in_the_dispatch_snapshot() {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 4, 4);
    assert_eq!(int_write(world, id, -1, "_OutputWidth", 2048).code, 0);
    assert_eq!(int_write(world, id, 0, "_OutputHeight", 1024).code, 0);
    let (name, name_length) = text("_Scale");
    let vector = [1.0_f32, 2.0, 3.0, 4.0];
    let mut matrix = [0.0_f32; 16];
    matrix[5] = 2.5;
    let (output_name, output_length) = text("_Output");
    unsafe {
        assert_eq!(
            disunity_compute_float_write(world, id, 0, name, name_length, 0.25).code,
            0
        );
        let (vname, vlength) = text("_Tint");
        assert_eq!(
            disunity_compute_vector_write(world, id, 0, vname, vlength, vector.as_ptr()).code,
            0
        );
        let (mname, mlength) = text("_Matrix");
        assert_eq!(
            disunity_compute_matrix_write(world, id, 0, mname, mlength, matrix.as_ptr()).code,
            0
        );
        let slot = DisunityTextureSlot {
            kind: 1,
            texture: (1 << 40) + 3,
            scale_x: 1.0,
            scale_y: 1.0,
            offset_x: 0.5,
            offset_y: 0.0,
        };
        let (tname, tlength) = text("_Albedo");
        assert_eq!(
            disunity_compute_texture_write(world, id, 0, tname, tlength, &slot).code,
            0
        );
        let global = DisunityComputeGlobalTexture {
            name_length: 8,
            global_length: 15,
        };
        let names = "_HeightsGlobalHeightmap";
        assert_eq!(
            disunity_compute_texture_global_write(world, id, 0, &global, names.as_ptr()).code,
            0
        );
        assert_eq!(
            disunity_compute_buffer_bind(world, id, 0, output_name, output_length, out).code,
            0
        );
        assert_eq!(disunity_compute_dispatch(world, id, 0, 256, 128, 1).code, 0);
    }
    let records = take(world);
    assert_eq!(records.len(), 1);
    let record = records[0];
    assert_eq!(record.kind, 3);
    assert_eq!((record.shader, record.kernel), (id, 0));
    assert_eq!(
        (record.groups_x, record.groups_y, record.groups_z),
        (256, 128, 1)
    );
    assert_ne!(record.bindings, 0);

    let entries = read_bindings(world, record.bindings);
    let find = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap();
    assert_eq!(find("_OutputWidth").entry.int_value, 2048);
    assert_eq!(find("_OutputHeight").entry.int_value, 1024);
    assert_eq!(find("_Scale").floats, [0.25]);
    assert_eq!(find("_Tint").floats, vector);
    assert_eq!(find("_Matrix").floats, matrix);
    let albedo = find("_Albedo");
    assert_eq!(
        (albedo.entry.texture_kind, albedo.entry.resource),
        (1, (1 << 40) + 3)
    );
    assert_eq!(albedo.floats, [1.0, 1.0, 0.5, 0.0]);
    let heights = find("_Heights");
    assert_eq!(heights.entry.texture_kind, 3);
    assert_eq!(heights.global, "GlobalHeightmap");
    let bound = find("_Output");
    assert_eq!((bound.entry.kind, bound.entry.resource), (5, out));
}

#[test]
fn a_dispatch_snapshot_is_unaffected_by_later_binding_edits() {
    let world = new_world();
    let id = shader(world);
    assert_eq!(int_write(world, id, -1, "_N", 1).code, 0);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    assert_eq!(int_write(world, id, -1, "_N", 2).code, 0);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let records = take(world);
    let values: Vec<i32> = records
        .iter()
        .map(|record| {
            let entries = read_bindings(world, record.bindings);
            entries
                .iter()
                .find(|e| e.name == "_N")
                .unwrap()
                .entry
                .int_value
        })
        .collect();
    assert_eq!(values, [1, 2]);
}

#[test]
fn binding_tokens_are_valid_until_the_next_take() {
    let world = new_world();
    let id = shader(world);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let first = take(world)[0].bindings;
    let mut info = DisunityComputeBindingsInfo::default();
    unsafe {
        let still = disunity_compute_bindings_read(
            world,
            first,
            &mut info,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(still.code, 0);
    }
    assert_eq!(disunity_compute_dispatch(world, id, 1, 2, 2, 2).code, 0);
    let second = take(world);
    assert_eq!(second.len(), 1);
    for token in [0, 2, u32::MAX] {
        let result = unsafe {
            disunity_compute_bindings_read(
                world,
                token,
                &mut info,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_ne!(result.code, 0, "{token}");
    }
}

#[test]
fn dispatch_binding_and_value_calls_are_validated() {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 1, 4);
    let (name, length) = text("_Output");
    unsafe {
        assert_ne!(disunity_compute_dispatch(world, id, 2, 1, 1, 1).code, 0);
        assert_ne!(disunity_compute_dispatch(world, id, 0, 0, 1, 1).code, 0);
        assert_ne!(disunity_compute_dispatch(world, id + 1, 0, 1, 1, 1).code, 0);
        assert_ne!(
            disunity_compute_buffer_bind(world, id, 5, name, length, out).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_bind(world, id, 0, name, length, out + 1).code,
            0
        );
        assert_ne!(
            disunity_compute_buffer_bind(world, id, 0, std::ptr::null(), 4, out).code,
            0
        );
        assert_ne!(
            disunity_compute_float_write(world, id, 0, name, length, f32::NAN).code,
            0
        );
        assert_ne!(
            disunity_compute_float_write(world, id, 7, name, length, 1.0).code,
            0
        );
        assert_ne!(
            disunity_compute_float_write(world, id, -2, name, length, 1.0).code,
            0
        );
        assert_ne!(
            disunity_compute_vector_write(world, id, 0, name, length, std::ptr::null()).code,
            0
        );
        assert_ne!(
            disunity_compute_matrix_write(world, id, 0, name, length, std::ptr::null()).code,
            0
        );
        assert_ne!(
            disunity_compute_texture_write(world, id, 0, name, length, std::ptr::null()).code,
            0
        );
        let slot = DisunityTextureSlot {
            kind: 3,
            ..Default::default()
        };
        assert_ne!(
            disunity_compute_texture_write(world, id, 0, name, length, &slot).code,
            0
        );
        let global = DisunityComputeGlobalTexture {
            name_length: 0,
            global_length: 4,
        };
        assert_ne!(
            disunity_compute_texture_global_write(world, id, 0, &global, name).code,
            0
        );
        assert_ne!(
            disunity_compute_texture_global_write(world, id, 0, std::ptr::null(), name).code,
            0
        );
    }
    assert!(take(world).is_empty());
}

#[test]
fn compute_dispatches_keep_their_place_among_blits() {
    let world = new_world();
    let id = shader(world);
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
        ..Default::default()
    };
    let mut target = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut target).code,
            0
        );
        let blit = DisunityOffscreenWork {
            target,
            pass: -1,
            ..work(1)
        };
        assert_eq!(disunity_offscreen_record(world, &blit).code, 0);
        assert_eq!(disunity_compute_dispatch(world, id, 0, 4, 1, 1).code, 0);
        assert_eq!(disunity_offscreen_record(world, &blit).code, 0);
    }
    let kinds: Vec<u32> = take(world).iter().map(|record| record.kind).collect();
    assert_eq!(kinds, [1, 3, 1]);
}

#[test]
fn a_dispatch_cannot_be_recorded_through_the_generic_record_call() {
    let world = new_world();
    let id = shader(world);
    let record = DisunityOffscreenWork {
        shader: id,
        groups_x: 1,
        groups_y: 1,
        groups_z: 1,
        ..work(3)
    };
    unsafe { assert_ne!(disunity_offscreen_record(world, &record).code, 0) };
    assert!(take(world).is_empty());
}

#[test]
fn a_global_texture_is_refused_by_materials_and_the_global_state() {
    let world = new_world();
    let slot = DisunityTextureSlot {
        kind: 3,
        ..Default::default()
    };
    let (name, length) = text("_Tex");
    unsafe {
        assert_ne!(
            disunity_shader_texture_write(world, 1, 0, name, length, &slot).code,
            0
        );
    }
    with_native_world(world, |native| {
        let held = native.global_shader_state().properties.texture("_Tex");
        assert_eq!(held.map(|slot| slot.texture), None::<TextureRef>);
    })
    .unwrap();
}

#[test]
fn compute_buffer_ids_in_the_world_match_the_ids_the_abi_hands_out() {
    let world = new_world();
    let id = buffer(world, 1, 4);
    with_native_world(world, |native| {
        assert!(
            native
                .compute_buffer(ComputeBufferId::from_bits(id))
                .is_ok()
        );
    })
    .unwrap();
}

struct Entry {
    entry: DisunityComputeBinding,
    name: String,
    global: String,
    floats: Vec<f32>,
}

fn read_bindings(world: u64, token: u32) -> Vec<Entry> {
    let mut info = DisunityComputeBindingsInfo::default();
    unsafe {
        let probe = disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(probe.code, 0);
    }
    let mut entries = vec![DisunityComputeBinding::default(); info.entry_count as usize];
    let mut floats = vec![0.0_f32; info.float_count as usize];
    let mut bytes = vec![0_u8; info.text_length as usize];
    unsafe {
        let read = disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            entries.as_mut_ptr(),
            floats.as_mut_ptr(),
            bytes.as_mut_ptr(),
        );
        assert_eq!(read.code, 0);
    }
    let slice = |offset: u32, length: u32| {
        String::from_utf8(bytes[offset as usize..(offset + length) as usize].to_vec()).unwrap()
    };
    entries
        .iter()
        .map(|entry| {
            let count = match entry.kind {
                0 | 1 | 5 => usize::from(entry.kind == 0),
                2 => 4,
                3 => 16,
                _ => 4,
            };
            let start = entry.float_offset as usize;
            Entry {
                entry: *entry,
                name: slice(entry.name_offset, entry.name_length),
                global: slice(entry.global_offset, entry.global_length),
                floats: floats[start..start + count].to_vec(),
            }
        })
        .collect()
}
