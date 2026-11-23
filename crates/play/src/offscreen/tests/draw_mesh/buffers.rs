//! A material's structured buffers in a mesh draw: a compute buffer bound to the material under
//! `_Properties` is the read-only storage buffer a vertex stage indexes by instance, as a unit
//! icon's instanced draw reads its per-instance properties.
//!
//! The vertex stage puts instance `i` of a mesh over clip space in the `i`th of the target's four
//! columns, so each test predicts the texel of every column from the colours the buffer holds.

use std::collections::BTreeMap;

use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, VERTEX, binding, shader as synthetic_shader, slot,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::graphics::ComputeBufferId;

use crate::offscreen::Settling;

use super::super::flush::{HEIGHT, scaling};
use super::super::kernels::{copy_object, write_compute};
use super::super::support::{compute_buffer, dispatch};
use super::{
    GREEN, MESH, Mat4, MaterialId, OffscreenWork, PlayerKernels, Rectangle, TargetRef, World,
    bind_and_clear, flush, flush_in, host, identity_view, mesh_of, record, scene, shader, snapshot,
    stored, target,
};

const PROPERTIES: &str = "_Properties";

const FIRST: [u8; 4] = [200, 40, 90, 255];
const SECOND: [u8; 4] = [30, 220, 60, 255];
const THIRD: [u8; 4] = [90, 70, 250, 255];
/// What a channel of 0.25 stores in the 8-bit target: 63.75, rounded.
const QUARTER: u8 = 64;

const WGSL: &str = "
@group(0) @binding(0) var<storage, read> props: array<vec4<f32>>;
struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
}
@vertex fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @builtin(instance_index) i: u32,
) -> VOut {
    var out: VOut;
    let x = (position.x * 0.5 + 0.5 + f32(i)) * 0.5 - 1.0;
    out.position = vec4<f32>(x, position.y, position.z, 1.0);
    out.color = props[i];
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return in.color; }";

/// The shader of the file `instanced`: one pass of [`WGSL`], whose `_Properties` is a structured
/// buffer of the vertex stage.
pub(super) fn instanced_passes() -> SyntheticShader {
    let mut spec = variant(
        &[],
        WGSL,
        "fs_main",
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: vec![binding(
                PROPERTIES,
                slot(0, 0, VERTEX),
                ResourceKind::Buffer,
            )],
        },
    );
    spec.channels = POSITION_AND_UV.to_vec();
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![spec],
        }],
    ))
}

/// A mesh of one rectangle over all of clip space.
fn whole() -> super::MeshChannels {
    mesh_of(&[Rectangle {
        x: (-1.0, 1.0),
        y: (-1.0, 1.0),
    }])
}

/// The bytes of `colors` as a buffer of vec4 floats holds them.
fn color_bytes(colors: &[[u8; 4]]) -> Vec<u8> {
    colors
        .iter()
        .flatten()
        .flat_map(|&byte| (f32::from(byte) / 255.0).to_le_bytes())
        .collect()
}

/// A compute buffer holding `colors`, bound to a new material of the `instanced` shader under
/// `_Properties`.
fn material_over(world: &mut World, colors: &[[u8; 4]]) -> (MaterialId, ComputeBufferId) {
    let bytes = color_bytes(colors);
    let count = u32::try_from(bytes.len() / 4).unwrap();
    let buffer = world.create_compute_buffer(count, 4, 0).unwrap();
    world.set_compute_buffer_data(buffer, 0, &bytes).unwrap();
    let shader = shader(world, "instanced");
    let material = world.create_material(shader).unwrap();
    world
        .set_material_buffer(material, PROPERTIES.into(), Some(buffer))
        .unwrap();
    (material, buffer)
}

fn draw_instances(world: &mut World, material: MaterialId, instances: u32) {
    record(
        world,
        OffscreenWork::DrawMesh {
            mesh: MESH,
            matrix: Mat4::IDENTITY.to_cols_array(),
            material,
            submesh: 0,
            pass: -1,
            block: None,
            instances,
        },
    );
}

#[test]
fn three_instances_read_the_three_colours_of_the_materials_buffer() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let (material, _) = material_over(&mut world, &[FIRST, SECOND, THIRD]);
    draw_instances(&mut world, material, 3);

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    // The fourth column is past the last instance and keeps the clear.
    assert_eq!(run.columns(), [FIRST, SECOND, THIRD, GREEN]);
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert!(run.outcome.report.missing_buffers.is_empty());
    assert!(
        run.outcome.writes.is_empty(),
        "a draw only reads the buffer"
    );
}

#[test]
fn a_buffer_destroyed_after_it_was_bound_leaves_the_draw_out_by_the_name_of_the_property() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let (gone, buffer) = material_over(&mut world, &[FIRST, SECOND, THIRD]);
    let (kept, _) = material_over(&mut world, &[THIRD, SECOND, FIRST]);
    world.destroy_compute_buffer(buffer).unwrap();
    draw_instances(&mut world, gone, 3);
    draw_instances(&mut world, kept, 2);

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    // The draw of the destroyed buffer is left out and the draw after it still runs.
    assert_eq!(run.columns(), [THIRD, SECOND, GREEN, GREEN]);
    assert_eq!(
        run.outcome.report.missing_buffers,
        [(PROPERTIES.to_owned(), 1)].into()
    );
}

#[test]
fn a_draw_after_a_dispatch_reads_what_the_dispatch_wrote_to_the_buffer_both_bind() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut world = super::world();
    let heights = target(&mut world, HEIGHT);
    // The kernel scales every height by 2, so each of the 16 values it writes is 0.25.
    record(
        &mut world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(heights)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
    record(
        &mut world,
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [0.125, 0.0, 0.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, 16);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let drawn = target(&mut world, super::ARGB32);
    bind_and_clear(&mut world, drawn);
    identity_view(&mut world);
    let material_shader = super::shader(&mut world, "instanced");
    let material = world.create_material(material_shader).unwrap();
    world
        .set_material_buffer(material, PROPERTIES.into(), Some(buffer))
        .unwrap();
    // Four instances read the four vec4 the sixteen floats make: all of them 0.25.
    draw_instances(&mut world, material, 4);
    let mut host = host(PlayerKernels::new(directory.path()));

    let run = flush_in(&mut host, &mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(run.columns(), [[QUARTER; 4]; 4]);
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    // The buffer was uploaded once for the dispatch and the draw, and the dispatch's result is
    // the one write.
    assert_eq!(run.outcome.writes.len(), 1);
    assert_eq!(run.outcome.writes[0].id, buffer);
}

#[test]
fn a_frame_binds_the_materials_buffer_and_counts_the_property_of_one_that_is_gone() {
    let meshes = stored([(MESH, whole())]);
    for destroyed in [false, true] {
        let (mut world, _) = scene();
        identity_view(&mut world);
        let (material, buffer) = material_over(&mut world, &[FIRST, SECOND, THIRD]);
        if destroyed {
            world.destroy_compute_buffer(buffer).unwrap();
        }
        draw_instances(&mut world, material, 3);
        let directory = tempfile::tempdir().unwrap();
        let mut host = host(PlayerKernels::new(directory.path()));

        let (operations, report) = host
            .frame(&snapshot(&mut world), &meshes, Settling::Wait, &[])
            .unwrap();

        assert!(operations.is_some(), "destroyed {destroyed}");
        let left_out: BTreeMap<_, _> = if destroyed {
            [(PROPERTIES.to_owned(), 1)].into()
        } else {
            BTreeMap::new()
        };
        assert_eq!(report.missing_buffers, left_out, "destroyed {destroyed}");
    }
}
