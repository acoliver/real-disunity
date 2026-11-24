//! Compute bindings and recorded dispatches through the ABI: what a dispatch captures, how
//! the frame's work carries it and that bad calls are refused.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{
    abi::*, compute::*, graphics::*, graphics_offscreen::*, graphics_properties::*,
    with_native_world,
};
use worldrunner_world_simulation::graphics::{
    ComputeBindings, ComputeBufferId, OffscreenWork, TextureRef,
};

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

/// One dispatch of the frame: the shader, kernel and thread groups it launched and the values
/// the kernel ran with.
struct Launch {
    shader: u64,
    kernel: u32,
    groups: [u32; 3],
    bindings: ComputeBindings,
}

/// The frame's dispatch records. A global written directly before one is ordered ahead of it as
/// a record of its own, which these tests leave to the world's `ordered_globals.rs`.
fn dispatches(world: u64) -> Vec<Launch> {
    snapshot(world)
        .work
        .into_iter()
        .filter_map(|record| match record {
            OffscreenWork::Dispatch {
                shader,
                kernel,
                groups,
                bindings,
            } => Some(Launch {
                shader: shader.to_bits(),
                kernel,
                groups,
                bindings,
            }),
            _ => None,
        })
        .collect()
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
fn dispatched_with_values() -> (u64, u64, u64, Launch) {
    let world = new_world();
    let id = shader(world);
    let out = buffer(world, 4, 4);
    assert_eq!(int_write(world, id, -1, "_OutputWidth", 2048).code, 0);
    assert_eq!(int_write(world, id, 0, "_OutputHeight", 1024).code, 0);
    bind_kernel_values(world, id, out);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 256, 128, 1).code, 0);
    let mut records = dispatches(world);
    assert_eq!(records.len(), 1);
    (world, id, out, records.remove(0))
}

#[test]
fn a_dispatch_record_names_the_shader_kernel_and_groups() {
    let (_, id, _, record) = dispatched_with_values();
    assert_eq!((record.shader, record.kernel), (id, 0));
    assert_eq!(record.groups, [256, 128, 1]);
}

#[test]
fn values_and_buffers_bound_to_a_kernel_arrive_with_the_dispatch() {
    let (_, _, out, record) = dispatched_with_values();
    let held = &record.bindings.properties;
    assert_eq!(held.int("_OutputWidth"), Some(2048));
    assert_eq!(held.int("_OutputHeight"), Some(1024));
    assert_eq!(held.float("_Scale"), Some(0.25));
    assert_eq!(held.vector("_Tint"), Some(VECTOR));
    assert_eq!(held.matrix("_Matrix"), Some(matrix()));
    let albedo = held.texture("_Albedo").unwrap();
    assert_eq!(albedo.texture, TextureRef::Asset((1 << 40) + 3));
    assert_eq!((albedo.scale, albedo.offset), ([1.0, 1.0], [0.5, 0.0]));
    assert_eq!(
        held.texture("_Heights").unwrap().texture,
        TextureRef::Asset((1 << 40) + 9),
        "the record holds the texture the global state held at the dispatch"
    );
    assert_eq!(
        record.bindings.buffers,
        [("_Output".to_string(), ComputeBufferId::from_bits(out))]
    );
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
        let first =
            disunity_shader_texture_write(world, 1, 0, global, global_length, &asset(1 << 40));
        assert_eq!(first.code, 0);
        assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
        let second =
            disunity_shader_texture_write(world, 1, 0, global, global_length, &asset(2 << 40));
        assert_eq!(second.code, 0);
    }
    let held: Vec<TextureRef> = dispatches(world)
        .iter()
        .map(|record| {
            record
                .bindings
                .properties
                .texture("_Heights")
                .unwrap()
                .texture
        })
        .collect();
    assert_eq!(held, [TextureRef::None, TextureRef::Asset(1 << 40)]);
}

#[test]
fn a_global_written_after_the_last_record_is_taken_after_it() {
    let world = new_world();
    let id = shader(world);
    let (global, length) = text("_Late");
    let slot = DisunityTextureSlot {
        kind: 1,
        texture: 1 << 40,
        scale_x: 1.0,
        scale_y: 1.0,
        ..Default::default()
    };
    unsafe {
        assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
        let late = disunity_shader_texture_write(world, 1, 0, global, length, &slot);
        assert_eq!(late.code, 0);
    }
    let records = snapshot(world).work;
    assert_eq!(records.len(), 2);
    assert!(matches!(records[0], OffscreenWork::Dispatch { .. }));
    assert!(matches!(records[1], OffscreenWork::SetGlobal(_)));
}

#[test]
fn a_dispatch_snapshot_is_unaffected_by_later_binding_edits() {
    let world = new_world();
    let id = shader(world);
    assert_eq!(int_write(world, id, -1, "_N", 1).code, 0);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    assert_eq!(int_write(world, id, -1, "_N", 2).code, 0);
    assert_eq!(disunity_compute_dispatch(world, id, 0, 1, 1, 1).code, 0);
    let values: Vec<Option<i32>> = dispatches(world)
        .iter()
        .map(|record| record.bindings.properties.int("_N"))
        .collect();
    assert_eq!(values, [Some(1), Some(2)]);
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
    assert!(snapshot(world).work.is_empty());
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
    assert!(snapshot(world).work.is_empty());
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
        dimension: 2,
        volume_depth: 1,
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
    let kinds: Vec<&str> = snapshot(world)
        .work
        .iter()
        .map(|record| match record {
            OffscreenWork::Blit { .. } => "blit",
            OffscreenWork::Dispatch { .. } => "dispatch",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["blit", "dispatch", "blit"]);
}

#[test]
fn a_dispatch_cannot_be_recorded_through_the_generic_record_call() {
    let world = new_world();
    let record = work(3);
    unsafe { assert_ne!(disunity_offscreen_record(world, &record).code, 0) };
    assert!(snapshot(world).work.is_empty());
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
