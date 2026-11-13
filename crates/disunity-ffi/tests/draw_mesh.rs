//! A command buffer's mesh draw through the ABI: the call forwards the stored mesh's identity,
//! matrix, material, sub-mesh and pass as one record, and a take hands it back with kind 16 and
//! the bytes that carry it. Property values written to the draw scope (3) before the call travel
//! with that draw only. An identity nothing stored, a missing or non-finite matrix, a dead
//! material and a pass below -1 are refused and record nothing.
use worldrunner_disunity_ffi::graphics_properties::*;
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_context::*, graphics_offscreen::*,
    meshes::*,
};
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, PropertyValue, ShaderProperties, snapshot_wire::decode_work,
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
            disunity_shader_create(world, b"Portrait".as_ptr(), 8, &mut shader).code,
            0
        );
        assert_eq!(
            disunity_material_create(world, shader, &mut material).code,
            0
        );
    }
    material
}

/// Stores a one-triangle mesh under `identity`.
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
        assert_eq!(
            disunity_mesh_store(
                world,
                identity,
                &info,
                vertices.as_ptr(),
                indices.as_ptr(),
                submeshes.as_ptr()
            )
            .code,
            0
        );
    }
}

fn take(world: u64) -> Vec<(DisunityOffscreenWork, OffscreenWork)> {
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 4];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 4, &mut count).code,
            0
        );
    }
    records[..count as usize]
        .iter()
        .map(|record| {
            let (mut length, mut bytes) = (0, [0_u8; 256]);
            unsafe {
                assert_eq!(
                    disunity_offscreen_detail(
                        world,
                        record.bindings,
                        bytes.as_mut_ptr(),
                        256,
                        &mut length
                    )
                    .code,
                    0
                );
            }
            (*record, decode_work(&bytes[..length as usize]).unwrap())
        })
        .collect()
}

#[test]
fn every_field_reaches_the_frame_as_one_record_of_kind_sixteen() {
    let world = new_world();
    let material = material(world);
    store_mesh(world, FIRST_PLAYER_MESH + 3);
    let matrix: [f32; 16] = std::array::from_fn(|i| 1.0 + i as f32);
    let code = unsafe {
        disunity_offscreen_draw_mesh(
            world,
            FIRST_PLAYER_MESH + 3,
            matrix.as_ptr(),
            material,
            4,
            -1,
        )
    }
    .code;
    assert_eq!(code, 0);
    let taken = take(world);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0.kind, KIND_DRAW_MESH);
    assert_eq!(KIND_DRAW_MESH, 16);
    assert_eq!(
        taken[0].1,
        OffscreenWork::DrawMesh {
            mesh: FIRST_PLAYER_MESH + 3,
            matrix,
            material: MaterialId::from_bits(material),
            submesh: 4,
            pass: -1,
            block: None,
        }
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_draw_the_world_or_the_store_refuses_records_nothing() {
    let world = new_world();
    let material = material(world);
    store_mesh(world, FIRST_PLAYER_MESH);
    let good = [0.0_f32; 16];
    let mut nan = good;
    nan[5] = f32::NAN;
    let mesh = FIRST_PLAYER_MESH;
    unsafe {
        let refused = [
            disunity_offscreen_draw_mesh(world, mesh, std::ptr::null(), material, 0, 0),
            disunity_offscreen_draw_mesh(world, mesh, nan.as_ptr(), material, 0, 0),
            disunity_offscreen_draw_mesh(world, mesh + 1, good.as_ptr(), material, 0, 0),
            disunity_offscreen_draw_mesh(world, 0, good.as_ptr(), material, 0, 0),
            disunity_offscreen_draw_mesh(world, mesh, good.as_ptr(), material + 1_000, 0, 0),
            disunity_offscreen_draw_mesh(world, mesh, good.as_ptr(), material, 0, -2),
            disunity_offscreen_draw_mesh(world + 999, mesh, good.as_ptr(), material, 0, 0),
        ];
        for (index, result) in refused.iter().enumerate() {
            assert_ne!(result.code, 0, "case {index}");
        }
    }
    assert!(take(world).is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

const SCOPE_DRAW: u32 = 3;

fn stage_float(world: u64, name: &str, value: f32) -> i32 {
    let length = u32::try_from(name.len()).unwrap();
    unsafe { disunity_shader_float_write(world, SCOPE_DRAW, 0, name.as_ptr(), length, value) }.code
}

fn draw(world: u64, material: u64, pass: i32) -> i32 {
    let matrix = [0.0_f32; 16];
    unsafe {
        disunity_offscreen_draw_mesh(world, FIRST_PLAYER_MESH, matrix.as_ptr(), material, 0, pass)
    }
    .code
}

fn block_of(record: &OffscreenWork) -> Option<ShaderProperties> {
    match record {
        OffscreenWork::DrawMesh { block, .. } => block.as_deref().cloned(),
        other => panic!("expected a mesh draw, got {other:?}"),
    }
}

fn floats(values: &[(&str, f32)]) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    for (name, value) in values {
        block
            .set(PropertyValue::Float((*name).into(), *value))
            .unwrap();
    }
    block
}

#[test]
fn values_staged_in_the_draw_scope_travel_with_the_next_draw_only() {
    let world = new_world();
    let material = material(world);
    store_mesh(world, FIRST_PLAYER_MESH);
    assert_eq!(stage_float(world, "_Fade", 0.5), 0);
    assert_eq!(stage_float(world, "_Gain", 2.0), 0);
    assert_eq!(stage_float(world, "_Fade", 0.75), 0);
    assert_eq!(draw(world, material, 0), 0);
    assert_eq!(draw(world, material, 0), 0);
    let taken = take(world);
    assert_eq!(taken.len(), 2);
    assert_eq!(
        block_of(&taken[0].1),
        Some(floats(&[("_Fade", 0.75), ("_Gain", 2.0)]))
    );
    assert_eq!(
        block_of(&taken[1].1),
        None,
        "the second draw staged nothing"
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_cleared_or_refused_draw_leaves_nothing_staged() {
    let world = new_world();
    let material = material(world);
    store_mesh(world, FIRST_PLAYER_MESH);
    assert_eq!(stage_float(world, "_Fade", 0.5), 0);
    assert_eq!(
        disunity_shader_properties_clear(world, SCOPE_DRAW, 0).code,
        0
    );
    assert_eq!(draw(world, material, 0), 0);
    assert_eq!(stage_float(world, "_Fade", 0.5), 0);
    assert_ne!(draw(world, material, -2), 0, "a pass below -1 is refused");
    assert_eq!(draw(world, material, 0), 0);
    assert_eq!(stage_float(world, "_Gain", 0.5), 0);
    let missing = unsafe {
        disunity_offscreen_draw_mesh(world, FIRST_PLAYER_MESH, std::ptr::null(), material, 0, 0)
    };
    assert_ne!(missing.code, 0, "a draw without a matrix is refused");
    assert_eq!(draw(world, material, 0), 0);
    let taken = take(world);
    assert_eq!(taken.len(), 3);
    assert_eq!(block_of(&taken[0].1), None, "the cleared block is empty");
    assert_eq!(
        block_of(&taken[1].1),
        None,
        "a refused draw consumes what was staged"
    );
    assert_eq!(
        block_of(&taken[2].1),
        None,
        "a draw refused for its matrix consumes it too"
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
