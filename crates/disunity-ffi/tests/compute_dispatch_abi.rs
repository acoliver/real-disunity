//! Compute bindings and recorded dispatches through the ABI: what a dispatch captures, how
//! its values are read back and that bad calls are refused.
use worldrunner_disunity_ffi::{
    abi::*, compute::*, compute_bindings::*, graphics::*, graphics_offscreen::*,
    graphics_properties::*, with_native_world,
};
use worldrunner_world_simulation::graphics::{ComputeBufferId, TextureRef};

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

fn int_write(world: u64, shader: u64, kernel: i32, name: &str, value: i32) -> DisunityResult {
    let (bytes, length) = text(name);
    unsafe { disunity_compute_int_write(world, shader, kernel, bytes, length, value) }
}

const VECTOR: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

fn matrix() -> [f32; 16] {
    let mut matrix = [0.0_f32; 16];
    matrix[5] = 2.5;
    matrix
}

/// Binds a float, vector, matrix, asset texture, global texture and buffer to kernel 0 of `id`.
fn bind_kernel_values(world: u64, id: u64, out: u64) {
    let (name, name_length) = text("_Scale");
    let (output_name, output_length) = text("_Output");
    let (vname, vlength) = text("_Tint");
    let (mname, mlength) = text("_Matrix");
    let (tname, tlength) = text("_Albedo");
    let slot = DisunityTextureSlot {
        kind: 1,
        texture: (1 << 40) + 3,
        scale_x: 1.0,
        scale_y: 1.0,
        offset_x: 0.5,
        offset_y: 0.0,
    };
    let global = DisunityComputeGlobalTexture {
        name_length: 8,
        global_length: 15,
    };
    let names = "_HeightsGlobalHeightmap";
    let (global_name, global_name_length) = text("GlobalHeightmap");
    let heightmap = DisunityTextureSlot {
        kind: 1,
        texture: (1 << 40) + 9,
        scale_x: 1.0,
        scale_y: 1.0,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    let matrix = matrix();
    unsafe {
        let results = [
            disunity_compute_float_write(world, id, 0, name, name_length, 0.25),
            disunity_compute_vector_write(world, id, 0, vname, vlength, VECTOR.as_ptr()),
            disunity_compute_matrix_write(world, id, 0, mname, mlength, matrix.as_ptr()),
            disunity_compute_texture_write(world, id, 0, tname, tlength, &slot),
            disunity_compute_texture_global_write(world, id, 0, &global, names.as_ptr()),
            disunity_shader_texture_write(world, 1, 0, global_name, global_name_length, &heightmap),
            disunity_compute_buffer_bind(world, id, 0, output_name, output_length, out),
        ];
        assert!(results.iter().all(|result| result.code == 0));
    }
}

/// A world whose kernel 0 has every kind of value bound and was dispatched once; returns the
/// world, the shader, the bound buffer and the dispatch record.
fn dispatched_with_values() -> (u64, u64, u64, DisunityOffscreenWork) {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 4, 4);
    assert_eq!(int_write(world, id, -1, "_OutputWidth", 2048).code, 0);
    assert_eq!(int_write(world, id, 0, "_OutputHeight", 1024).code, 0);
    bind_kernel_values(world, id, out);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 256, 128, 1).code, 0);
    let records = take(world);
    assert_eq!(records.len(), 1);
    (world, id, out, records[0])
}

#[test]
fn a_dispatch_record_names_the_shader_kernel_groups_and_a_binding_token() {
    let (_, id, _, record) = dispatched_with_values();
    assert_eq!(record.kind, 3);
    assert_eq!((record.shader, record.kernel), (id, 0));
    assert_eq!(
        (record.groups_x, record.groups_y, record.groups_z),
        (256, 128, 1)
    );
    assert_ne!(record.bindings, 0);
}

#[test]
fn values_and_buffers_bound_to_a_kernel_arrive_in_the_dispatch_snapshot() {
    let (world, _, out, record) = dispatched_with_values();
    let entries = read_bindings(world, record.bindings);
    let find = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap();
    assert_eq!(find("_OutputWidth").entry.int_value, 2048);
    assert_eq!(find("_OutputHeight").entry.int_value, 1024);
    assert_eq!(find("_Scale").floats, [0.25]);
    assert_eq!(find("_Tint").floats, VECTOR);
    assert_eq!(find("_Matrix").floats, matrix());
    let albedo = find("_Albedo");
    assert_eq!(
        (albedo.entry.texture_kind, albedo.entry.resource),
        (1, (1 << 40) + 3)
    );
    assert_eq!(albedo.floats, [1.0, 1.0, 0.5, 0.0]);
    let heights = find("_Heights");
    assert_eq!(
        (heights.entry.texture_kind, heights.entry.resource),
        (1, (1 << 40) + 9),
        "the record holds the texture the global state held at the dispatch"
    );
    let bound = find("_Output");
    assert_eq!((bound.entry.kind, bound.entry.resource), (5, out));
}

#[test]
fn a_global_texture_is_resolved_when_the_dispatch_is_called_and_not_when_it_is_read() {
    let world = new_world();
    let id = shader(world);
    let (global, global_length) = text("_Terrain");
    let names = "_Heights_Terrain";
    let lengths = DisunityComputeGlobalTexture {
        name_length: 8,
        global_length,
    };
    let asset = |texture: u64| DisunityTextureSlot {
        kind: 1,
        texture,
        scale_x: 1.0,
        scale_y: 1.0,
        ..Default::default()
    };
    unsafe {
        let bound = disunity_compute_texture_global_write(world, id, 0, &lengths, names.as_ptr());
        assert_eq!(bound.code, 0);
        assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
        let first = disunity_shader_texture_write(world, 1, 0, global, global_length, &asset(1 << 40));
        assert_eq!(first.code, 0);
        assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
        let second = disunity_shader_texture_write(world, 1, 0, global, global_length, &asset(2 << 40));
        assert_eq!(second.code, 0);
    }
    let held: Vec<(u32, u64)> = take(world)
        .iter()
        .map(|record| {
            let entries = read_bindings(world, record.bindings);
            let heights = entries.iter().find(|e| e.name == "_Heights").unwrap();
            (heights.entry.texture_kind, heights.entry.resource)
        })
        .collect();
    assert_eq!(held, [(0, 0), (1, 1 << 40)]);
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
fn dispatch_and_binding_calls_are_validated() {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 1, 4);
    let (name, length) = text("_Output");
    unsafe {
        let refused = [
            disunity_compute_dispatch(world, id, 2, 1, 1, 1),
            disunity_compute_dispatch(world, id, 0, 0, 1, 1),
            disunity_compute_dispatch(world, id + 1, 0, 1, 1, 1),
            disunity_compute_buffer_bind(world, id, 5, name, length, out),
            disunity_compute_buffer_bind(world, id, 0, name, length, out + 1),
            disunity_compute_buffer_bind(world, id, 0, std::ptr::null(), 4, out),
        ];
        assert!(refused.iter().all(|result| result.code != 0));
    }
    assert!(take(world).is_empty());
}

#[test]
fn value_calls_are_validated() {
    let world = new_world();
    let id = shader(world);
    let (name, length) = text("_Output");
    let slot = DisunityTextureSlot {
        kind: 3,
        ..Default::default()
    };
    let global = DisunityComputeGlobalTexture {
        name_length: 0,
        global_length: 4,
    };
    unsafe {
        let refused = [
            disunity_compute_float_write(world, id, 0, name, length, f32::NAN),
            disunity_compute_float_write(world, id, 7, name, length, 1.0),
            disunity_compute_float_write(world, id, -2, name, length, 1.0),
            disunity_compute_vector_write(world, id, 0, name, length, std::ptr::null()),
            disunity_compute_matrix_write(world, id, 0, name, length, std::ptr::null()),
            disunity_compute_texture_write(world, id, 0, name, length, std::ptr::null()),
            disunity_compute_texture_write(world, id, 0, name, length, &slot),
            disunity_compute_texture_global_write(world, id, 0, &global, name),
            disunity_compute_texture_global_write(world, id, 0, std::ptr::null(), name),
        ];
        assert!(refused.iter().all(|result| result.code != 0));
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
                floats: floats[start..start + count].to_vec(),
            }
        })
        .collect()
}

#[test]
fn bindings_without_floats_are_read_with_no_place_for_floats() {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 1, 4);
    let (name, length) = text("_Output");
    unsafe {
        assert_eq!(
            disunity_compute_buffer_bind(world, id, 0, name, length, out).code,
            0
        )
    };
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let token = take(world)[0].bindings;
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
    assert_eq!((info.entry_count, info.float_count), (1, 0));
    let mut entries = vec![DisunityComputeBinding::default(); 1];
    let mut bytes = vec![0_u8; info.text_length as usize];
    unsafe {
        let read = disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            entries.as_mut_ptr(),
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
        );
        assert_eq!(read.code, 0);
        assert_eq!(&bytes[..7], b"_Output");
        let no_text = disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            entries.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_ne!(no_text.code, 0);
    }
}

#[test]
fn bindings_with_floats_need_a_place_for_them() {
    let world = new_world();
    let id = shader(world);
    let (name, length) = text("_Scale");
    unsafe {
        assert_eq!(
            disunity_compute_float_write(world, id, -1, name, length, 2.0).code,
            0
        )
    };
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let token = take(world)[0].bindings;
    let mut info = DisunityComputeBindingsInfo {
        entry_count: 4,
        float_count: 4,
        text_length: 64,
    };
    let mut entries = vec![DisunityComputeBinding::default(); 4];
    let mut bytes = vec![0_u8; 64];
    let result = unsafe {
        disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            entries.as_mut_ptr(),
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
        )
    };
    assert_ne!(result.code, 0);
}

/// A dispatch with a buffer and a float bound, and the sizes its bindings read back with.
fn dispatch_with_two_entries(world: u64) -> (u32, DisunityComputeBindingsInfo) {
    let id = shader(world);
    let out = buffer(world, 1, 4);
    let (name, length) = text("_Output");
    let (scale, scale_length) = text("_Scale");
    unsafe {
        let set = [
            disunity_compute_buffer_bind(world, id, 0, name, length, out),
            disunity_compute_float_write(world, id, -1, scale, scale_length, 2.0),
        ];
        assert!(set.iter().all(|result| result.code == 0));
    }
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let token = take(world)[0].bindings;
    let mut held = DisunityComputeBindingsInfo::default();
    unsafe {
        let probe = disunity_compute_bindings_read(
            world,
            token,
            &mut held,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(probe.code, 0);
    }
    (token, held)
}

fn read_with(world: u64, token: u32, mut info: DisunityComputeBindingsInfo) -> DisunityResult {
    let mut entries = vec![DisunityComputeBinding::default(); 8];
    let mut floats = vec![0.0_f32; 8];
    let mut bytes = vec![0_u8; 64];
    unsafe {
        disunity_compute_bindings_read(
            world,
            token,
            &mut info,
            entries.as_mut_ptr(),
            floats.as_mut_ptr(),
            bytes.as_mut_ptr(),
        )
    }
}

#[test]
fn bindings_are_refused_when_any_capacity_is_one_short() {
    let world = new_world();
    let (token, held) = dispatch_with_two_entries(world);
    assert_eq!((held.entry_count, held.float_count), (2, 1));
    let short = [
        DisunityComputeBindingsInfo {
            entry_count: held.entry_count - 1,
            ..held
        },
        DisunityComputeBindingsInfo {
            float_count: held.float_count - 1,
            ..held
        },
        DisunityComputeBindingsInfo {
            text_length: held.text_length - 1,
            ..held
        },
    ];
    for info in short {
        assert_ne!(read_with(world, token, info).code, 0, "{info:?}");
    }
    assert_eq!(read_with(world, token, held).code, 0);
}
