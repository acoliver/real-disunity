//! A command buffer's procedural draw through the ABI: the call forwards the matrix, material,
//! pass, topology and counts as one record, and the frame's work holds it as one procedural draw.
//! A missing or non-finite matrix, a dead material, a pass below -1, an unknown topology and a
//! negative count are refused and record nothing.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{abi::*, graphics::*, graphics_context::*};
use worldrunner_world_simulation::graphics::{DrawTopology, MaterialId, OffscreenWork};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn material(world: u64) -> u64 {
    let (mut shader, mut material) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_shader_create(world, b"Generated".as_ptr(), 9, &mut shader).code,
            0
        );
        assert_eq!(
            disunity_material_create(world, shader, &mut material).code,
            0
        );
    }
    material
}

#[test]
fn every_field_reaches_the_frame_as_procedural_draws() {
    let world = new_world();
    let material = material(world);
    let matrix: [f32; 16] = std::array::from_fn(|i| 0.5 - i as f32);
    let codes = unsafe {
        [
            disunity_offscreen_draw_procedural(world, matrix.as_ptr(), material, 2, 1, 6, 3),
            disunity_offscreen_draw_procedural(world, matrix.as_ptr(), material, -1, 0, 0, 1),
        ]
    };
    assert_eq!(codes.map(|result| result.code), [0, 0]);
    assert_eq!(
        snapshot(world).work,
        [
            OffscreenWork::DrawProcedural {
                matrix,
                material: MaterialId::from_bits(material),
                pass: 2,
                topology: DrawTopology::LineList,
                vertex_count: 6,
                instance_count: 3,
            },
            OffscreenWork::DrawProcedural {
                matrix,
                material: MaterialId::from_bits(material),
                pass: -1,
                topology: DrawTopology::TriangleList,
                vertex_count: 0,
                instance_count: 1,
            },
        ]
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_draw_the_abi_or_the_world_refuses_records_nothing() {
    let world = new_world();
    let material = material(world);
    let good = [0.0_f32; 16];
    let mut nan = good;
    nan[9] = f32::INFINITY;
    unsafe {
        let refused = [
            disunity_offscreen_draw_procedural(world, std::ptr::null(), material, 0, 0, 3, 1),
            disunity_offscreen_draw_procedural(world, nan.as_ptr(), material, 0, 0, 3, 1),
            disunity_offscreen_draw_procedural(world, good.as_ptr(), material + 1_000, 0, 0, 3, 1),
            disunity_offscreen_draw_procedural(world, good.as_ptr(), material, -2, 0, 3, 1),
            disunity_offscreen_draw_procedural(world, good.as_ptr(), material, 0, 2, 3, 1),
            disunity_offscreen_draw_procedural(world, good.as_ptr(), material, 0, 0, -1, 1),
            disunity_offscreen_draw_procedural(world, good.as_ptr(), material, 0, 0, 3, -1),
            disunity_offscreen_draw_procedural(world + 999, good.as_ptr(), material, 0, 0, 3, 1),
        ];
        for (index, result) in refused.iter().enumerate() {
            assert_ne!(result.code, 0, "case {index}");
        }
    }
    assert!(snapshot(world).work.is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
