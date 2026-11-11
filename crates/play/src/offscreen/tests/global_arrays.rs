//! Vector and matrix array properties on a real adapter (lavapipe in CI): the arrays of the
//! initial global state, the arrays a command sets between two draws, and the arrays of a compute
//! dispatch reach the constant buffer of the pass or kernel that declares them.
//!
//! The blit shader declares `_Colors`, two vectors, and `_Bases`, two matrices. Pass 0 writes
//! `_Colors[0]` over the left half of the target and `_Colors[1]` over the right half. Pass 1
//! writes column 1 of `_Bases[0]` over the left half and column 2 of `_Bases[1]` over the right
//! half. No row of a test matrix equals the column the pass shows, so a transposed matrix and
//! elements in the wrong order both change the texels. Every expected value is computed from the
//! arrays the test sets.

use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, binding, constant_buffer, matrix_member,
    shader as synthetic_shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ResourceKind, VariantParameters,
};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeShaderId, MaterialId, OffscreenWork, PropertyValue, RenderTextureId,
    TextureRef,
};

use super::flush::host;
use super::gpu::{SIDE, shader, target};
use super::kernels::compute_bytes::{
    Binding, BufferLayout, Kernel, Member, Platform, SPIRV, Variant, compute_shader_bytes,
};
use super::kernels::write_compute;
use super::support::{compute_buffer, compute_shader, world};
use super::volumes::{code, failures, flush};

/// `GraphicsFormat` 8, four 8-bit channels.
const ARGB32: u32 = 8;
/// The elements each array member declares.
const DECLARED: i32 = 2;
const VECTORS: i32 = 0;
const COLUMNS: i32 = 1;

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const WHITE: [f32; 4] = [1.0; 4];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const CYAN: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const MAGENTA: [f32; 4] = [1.0, 0.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

const ARRAYS_BLIT: &str = "
struct Material { colors: array<vec4<f32>, 2>, bases: array<mat4x4<f32>, 2> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var main_tex: texture_2d<f32>;
@group(0) @binding(2) var main_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_vectors(in: VOut) -> @location(0) vec4<f32> {
    // The source is read with a weight of zero so that the pass binds what a blit supplies.
    let unused = textureSample(main_tex, main_sampler, in.uv) * 0.0;
    if in.uv.x < 0.5 {
        return unused + material.colors[0];
    }
    return unused + material.colors[1];
}
@fragment fn fs_columns(in: VOut) -> @location(0) vec4<f32> {
    let unused = textureSample(main_tex, main_sampler, in.uv) * 0.0;
    if in.uv.x < 0.5 {
        return unused + material.bases[0][1];
    }
    return unused + material.bases[1][2];
}";

/// `_Colors` at byte 0, one register for each vector, and `_Bases` at byte 32, four registers for
/// each matrix.
fn arrays_parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![constant_buffer(
            "Material",
            160,
            vec![
                BufferMember {
                    array_size: DECLARED,
                    ..vector_member("_Colors", 0)
                },
                BufferMember {
                    array_size: DECLARED,
                    ..matrix_member("_Bases", 32)
                },
            ],
        )],
        bindings: vec![
            binding(
                "Material",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                "_MainTex",
                slot(0, 1, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 2, FRAGMENT)),
                    dimension: 4,
                },
            ),
            binding("", slot(0, 2, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    }
}

/// The blit shader of the file `arrays`: pass 0 shows the vectors, pass 1 the matrix columns.
pub(super) fn arrays_blit_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &["fs_vectors", "fs_columns"].map(|fragment| PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], ARRAYS_BLIT, fragment, arrays_parameters())],
        }),
    ))
}

/// A matrix from its four columns, in the order the world stores it: element `column * 4 + row`.
fn matrix(columns: [[f32; 4]; 4]) -> [f32; 16] {
    std::array::from_fn(|index| columns[index / 4][index % 4])
}

/// Column 1 is red. Row 1 is blue.
fn first() -> [f32; 16] {
    matrix([BLUE, RED, GREEN, WHITE])
}

/// Column 2 is magenta. Row 2 is (0, 1, 1, 0).
fn second() -> [f32; 16] {
    matrix([YELLOW, CYAN, MAGENTA, BLACK])
}

/// A world with a blit source, a target and a material of the `arrays` shader that sets nothing.
fn scene() -> (World, (RenderTextureId, RenderTextureId), MaterialId) {
    let mut world = world();
    let targets = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    let shader = shader(&mut world, "arrays");
    let material = world.create_material(shader).unwrap();
    (world, targets, material)
}

/// Records a blit of `pass` into `drawn` and a read of the whole of `drawn` after it.
fn blit_and_read(
    world: &mut World,
    (source, drawn): (RenderTextureId, RenderTextureId),
    material: MaterialId,
    pass: i32,
) -> u64 {
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: drawn,
            material: Some(material),
            pass,
        })
        .unwrap();
    world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap()
}

fn set_global(world: &mut World, value: PropertyValue) {
    world
        .record_offscreen(OffscreenWork::SetGlobal(value))
        .unwrap();
}

fn colors(elements: &[[f32; 4]]) -> PropertyValue {
    PropertyValue::VectorArray("_Colors".into(), elements.to_vec())
}

fn bases(elements: &[[f32; 16]]) -> PropertyValue {
    PropertyValue::MatrixArray("_Bases".into(), elements.to_vec())
}

/// A read of the whole target whose left two columns hold `left` and whose right two hold `right`.
fn halves(left: [f32; 4], right: [f32; 4]) -> Vec<f32> {
    (0..SIDE)
        .flat_map(|_| {
            (0..SIDE).flat_map(move |column| if column < SIDE / 2 { left } else { right })
        })
        .collect()
}

#[test]
fn arrays_of_the_initial_globals_reach_a_draw_in_element_and_column_order() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let (mut world, targets, material) = scene();
    world.set_global_property(colors(&[RED, GREEN])).unwrap();
    world
        .set_global_property(bases(&[first(), second()]))
        .unwrap();
    let vectors = blit_and_read(&mut world, targets, material, VECTORS);
    let columns = blit_and_read(&mut world, targets, material, COLUMNS);

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(
        world.take_texture_pixels(vectors),
        Some(halves(RED, GREEN)),
        "element 0 on the left, element 1 on the right"
    );
    assert_eq!(
        world.take_texture_pixels(columns),
        Some(halves(RED, MAGENTA)),
        "column 1 of element 0 and column 2 of element 1"
    );
}

#[test]
fn a_vector_array_a_command_sets_reaches_the_draws_after_it_until_the_next_replaces_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let (mut world, targets, material) = scene();
    set_global(&mut world, colors(&[RED, GREEN]));
    let earlier = blit_and_read(&mut world, targets, material, VECTORS);
    set_global(&mut world, colors(&[BLUE, WHITE]));
    let later = blit_and_read(&mut world, targets, material, VECTORS);

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(world.take_texture_pixels(earlier), Some(halves(RED, GREEN)));
    assert_eq!(world.take_texture_pixels(later), Some(halves(BLUE, WHITE)));
}

#[test]
fn a_matrix_array_a_command_sets_reaches_the_draws_after_it_until_the_next_replaces_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let (mut world, targets, material) = scene();
    set_global(&mut world, bases(&[first(), second()]));
    let earlier = blit_and_read(&mut world, targets, material, COLUMNS);
    set_global(&mut world, bases(&[second(), first()]));
    let later = blit_and_read(&mut world, targets, material, COLUMNS);

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(
        world.take_texture_pixels(earlier),
        Some(halves(RED, MAGENTA))
    );
    assert_eq!(
        world.take_texture_pixels(later),
        Some(halves(CYAN, GREEN)),
        "column 1 of the second matrix and column 2 of the first"
    );
}

#[test]
fn an_array_longer_than_the_pass_declares_leaves_that_draw_out_by_name_and_no_other() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let (mut world, targets, material) = scene();
    set_global(&mut world, colors(&[RED, GREEN]));
    let fitting = blit_and_read(&mut world, targets, material, VECTORS);
    set_global(&mut world, colors(&[BLUE, WHITE, YELLOW]));
    let refused = blit_and_read(&mut world, targets, material, VECTORS);
    set_global(&mut world, colors(&[BLUE, WHITE]));
    let fitting_again = blit_and_read(&mut world, targets, material, VECTORS);

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("record:Constant", 1)]);
    assert_eq!(
        outcome.report.causes.get("record:Constant/ArrayTooLong"),
        Some(&1)
    );
    assert_eq!(
        outcome.report.examples["record:Constant/ArrayTooLong"],
        "property _Colors has 3 elements, its declaration 2"
    );
    assert_eq!(world.take_texture_pixels(fitting), Some(halves(RED, GREEN)));
    assert_eq!(
        world.take_texture_pixels(refused),
        Some(halves(RED, GREEN)),
        "the refused draw leaves the target as the draw before it did"
    );
    assert_eq!(
        world.take_texture_pixels(fitting_again),
        Some(halves(BLUE, WHITE))
    );
}

/// A kernel that copies its array constants into `_Output`, four floats for each row of thread
/// identities: `_Weights[0]`, `_Weights[1]`, column 1 of `_Bases[0]` and column 2 of `_Bases[1]`.
const ARRAYS_KERNEL: &str = "
struct Params { weights: array<vec4<f32>, 2>, bases: array<mat4x4<f32>, 2> }
@group(0) @binding(0) var<storage, read_write> output: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    var row = params.weights[0];
    if id.y == 1u { row = params.weights[1]; }
    if id.y == 2u { row = params.bases[0][1]; }
    if id.y == 3u { row = params.bases[1][2]; }
    output[id.y * 4u + id.x] = row[id.x];
}";

/// The compute shader object of [`ARRAYS_KERNEL`], whose kernel is `Copy`. A member's words are
/// its kind, byte offset, array size, rows and columns.
fn arrays_object() -> Vec<u8> {
    let mut variant = Variant::new(&code(ARRAYS_KERNEL), [2, 2, 1]);
    variant.constant_buffers = vec![Binding::constant_buffer("CArrays", 0, 1)];
    variant.writable = vec![Binding::writable("_Output", 0)];
    let mut vulkan = Platform::new(SPIRV, 0, vec![Kernel::single("Copy", variant)]);
    vulkan.buffers = vec![BufferLayout {
        name: "CArrays".to_owned(),
        size: 160,
        members: vec![
            Member {
                name: "_Weights".to_owned(),
                words: [0, 0, DECLARED, 1, 4],
            },
            Member {
                name: "_Bases".to_owned(),
                words: [0, 32, DECLARED, 4, 4],
            },
        ],
    }];
    compute_shader_bytes("Arrays", &[vulkan], false)
}

/// Sixteen floats that count up from `first`.
fn sequence(first: f32) -> [f32; 16] {
    let mut next = first;
    std::array::from_fn(|_| {
        let value = next;
        next += 1.0;
        value
    })
}

/// A dispatch of the arrays kernel over four by four thread identities with `weights` and the
/// matrices that count up from 100 and from 200, and the buffer it writes.
fn dispatch_arrays(world: &mut World, weights: &[[f32; 4]]) -> (ComputeShaderId, ComputeBufferId) {
    let shader = compute_shader(world, "arrays");
    let buffer = compute_buffer(world, SIDE * SIDE);
    for value in [
        PropertyValue::VectorArray("_Weights".into(), weights.to_vec()),
        PropertyValue::MatrixArray("_Bases".into(), vec![sequence(100.0), sequence(200.0)]),
    ] {
        world.set_compute_property(shader, None, value).unwrap();
    }
    world
        .bind_compute_buffer(shader, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(shader, 0, [2, 2, 1]).unwrap();
    (shader, buffer)
}

fn floats(world: &World, buffer: ComputeBufferId) -> Vec<f32> {
    world
        .compute_buffer(buffer)
        .unwrap()
        .contents
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

#[test]
fn the_arrays_of_a_dispatch_reach_the_kernel_and_its_trace_counts_their_elements() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "arrays", vec![(1, arrays_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let weights = [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]];
    let (_, buffer) = dispatch_arrays(&mut world, &weights);

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    // Column 1 of the matrix from 100 is its elements 4 to 7, and column 2 of the matrix from 200
    // its elements 8 to 11.
    assert_eq!(
        floats(&world, buffer),
        [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 104.0, 105.0, 106.0, 107.0, 208.0, 209.0,
            210.0, 211.0
        ]
    );
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace: {:?}", outcome.report.traces);
    };
    assert_eq!(trace.constants, ["_Weights=2 vectors", "_Bases=2 matrices"]);
    assert!(trace.constants_missing.is_empty());
    assert_eq!(trace.outcome, "ran");
}

#[test]
fn a_dispatch_whose_array_is_longer_than_the_kernel_declares_is_left_out_as_constants() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "arrays", vec![(1, arrays_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let (_, buffer) = dispatch_arrays(&mut world, &[[1.0; 4], [2.0; 4], [3.0; 4]]);

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:Constants", 1)]);
    assert!(outcome.writes.is_empty());
    assert_eq!(floats(&world, buffer), vec![0.0; 16]);
    assert_eq!(outcome.report.traces[0].outcome, "Constants");
}
