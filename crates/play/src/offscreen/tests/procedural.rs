//! Procedural draws on a real adapter (lavapipe in CI): a command buffer's `DrawProcedural` goes
//! from the world through the snapshot to the offscreen host, and a pixel read sees the texels.
//!
//! The synthetic vertex stage reads no vertex input. Vertex `v` is a corner of one of two
//! full-height rectangles, `[-1, 0]` for `v < 6` and `[0, 1]` for the next six, moved right by
//! one for each instance, then placed by the draw's `unity_ObjectToWorld` and `unity_MatrixVP`.
//! Pass 0 writes the material's `_Tint`, pass 1 writes element 1 of the global array `_Colors`.
//! The target is `SIDE` (4) texels square and cleared to green, so columns 0 and 1 are the left
//! rectangle and columns 2 and 3 the right one. Every expected texel follows from those
//! rectangles, the matrices and the colours the test sets.

use glam::{Mat4, Vec3};
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, binding, constant_buffer, matrix_member,
    shader as synthetic_shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ResourceKind, VariantParameters,
};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    DrawTopology, MaterialId, OffscreenWork, PropertyValue, RenderTextureId, TargetRef,
};

use super::super::flush::{FlushOutcome, OffscreenHost};
use super::flush::host;
use super::gpu::{SIDE, shader, target, vector};
use super::support::world;
use super::volumes::{failures, flush};

const ARGB32: u32 = 8;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

const WGSL: &str = "
struct Material { tint: vec4<f32>, colors: array<vec4<f32>, 2> }
struct Draw { object_to_world: mat4x4<f32>, view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
fn corner(v: u32) -> vec2<f32> {
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    return quad[v % 6u] + vec2<f32>(f32(v / 6u) - 1.0, 0.0);
}
@vertex fn vs_main(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32)
    -> @builtin(position) vec4<f32> {
    let p = corner(v) + vec2<f32>(f32(i), 0.0);
    return draw.view_projection * (draw.object_to_world * vec4<f32>(p, 0.0, 1.0));
}
@fragment fn fs_tint() -> @location(0) vec4<f32> { return material.tint; }
@fragment fn fs_array() -> @location(0) vec4<f32> { return material.colors[1]; }";

/// A vertex stage that reads a position input, and a fragment stage that samples `_Unset`, a
/// texture no material or global names.
const INPUTS_WGSL: &str = "
struct Material { tint: vec4<f32>, colors: array<vec4<f32>, 2> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(2) var unset: texture_2d<f32>;
@group(0) @binding(3) var unset_sampler: sampler;
@vertex fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs_tint(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return material.tint * textureSample(unset, unset_sampler, at.xy);
}";

fn parameters(with_draw: bool) -> VariantParameters {
    let material = constant_buffer(
        "Material",
        48,
        vec![
            vector_member("_Tint", 0),
            BufferMember {
                array_size: 2,
                ..vector_member("_Colors", 16)
            },
        ],
    );
    let mut parameters = VariantParameters {
        constant_buffers: vec![material],
        bindings: vec![binding(
            "Material",
            slot(0, 0, FRAGMENT),
            ResourceKind::ConstantBuffer,
        )],
    };
    if with_draw {
        parameters.constant_buffers.push(constant_buffer(
            "Draw",
            128,
            vec![
                matrix_member("unity_ObjectToWorld", 0),
                matrix_member("unity_MatrixVP", 64),
            ],
        ));
        parameters.bindings.push(binding(
            "Draw",
            slot(0, 1, VERTEX),
            ResourceKind::ConstantBuffer,
        ));
    }
    parameters
}

/// A pass of the generated-vertex stage of `WGSL`, which reads no vertex channel.
fn pass(fragment: &'static str) -> PassSpec {
    let mut spec = variant(&[], WGSL, fragment, parameters(true));
    spec.channels = Vec::new();
    PassSpec {
        tags: Vec::new(),
        variants: vec![spec],
    }
}

/// The shader of the file `procedural`: pass 0 writes `_Tint`, pass 1 element 1 of `_Colors`.
pub(super) fn procedural_passes() -> SyntheticShader {
    no_cull(synthetic_shader(&[], &[pass("fs_tint"), pass("fs_array")]))
}

/// The shader of the file `procedural_inputs`, whose vertex stage reads a position and whose
/// fragment stage samples the unset `_Unset`.
pub(super) fn input_reading_passes() -> SyntheticShader {
    let mut parameters = parameters(false);
    parameters.bindings.extend([
        binding(
            "_Unset",
            slot(0, 2, FRAGMENT),
            ResourceKind::Texture {
                sampler: Some(slot(0, 3, FRAGMENT)),
                dimension: 4,
            },
        ),
        binding("", slot(0, 3, FRAGMENT), ResourceKind::Sampler { state: 0 }),
    ]);
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], INPUTS_WGSL, "fs_tint", parameters)],
        }],
    ))
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

/// Binds `drawn`, clears it to green and sets an identity view and projection.
fn bind_and_clear(world: &mut World, drawn: RenderTextureId) {
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
    record(
        world,
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [0.0, 1.0, 0.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
    view(world, Mat4::IDENTITY);
}

fn view(world: &mut World, view_projection: Mat4) {
    record(
        world,
        OffscreenWork::SetViewProjection {
            view: Mat4::IDENTITY.to_cols_array(),
            projection: view_projection.to_cols_array(),
        },
    );
}

/// A material of the shader in `file` whose `_Tint` is red.
fn red(world: &mut World, file: &str) -> MaterialId {
    let shader = shader(world, file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", [1.0, 0.0, 0.0, 1.0]);
    material
}

fn draw(
    world: &mut World,
    (material, pass): (MaterialId, i32),
    (topology, matrix): (DrawTopology, Mat4),
    counts: (u32, u32),
) {
    record(
        world,
        OffscreenWork::DrawProcedural {
            matrix: matrix.to_cols_array(),
            material,
            pass,
            topology,
            vertex_count: counts.0,
            instance_count: counts.1,
        },
    );
}

/// The texels of each target after one flush, rows bottom first.
struct Run {
    targets: Vec<Vec<[u8; 4]>>,
    outcome: FlushOutcome,
}

impl Run {
    fn at(&self, target: usize, (column, row): (u32, u32)) -> [u8; 4] {
        self.targets[target][(row * SIDE + column) as usize]
    }

    /// The texel of every row of `column` of `target`, which must agree.
    fn column(&self, target: usize, column: u32) -> [u8; 4] {
        let first = self.at(target, (column, 0));
        for row in 0..SIDE {
            assert_eq!(
                self.at(target, (column, row)),
                first,
                "target {target} column {column} row {row}"
            );
        }
        first
    }

    /// [`Run::column`] of each column of `target`, left to right.
    fn columns(&self, target: usize) -> Vec<[u8; 4]> {
        (0..SIDE)
            .map(|column| self.column(target, column))
            .collect()
    }

    fn clean(&self) {
        assert!(
            failures(&self.outcome).is_empty(),
            "{:?}",
            failures(&self.outcome)
        );
        assert!(
            self.outcome.report.skipped.is_empty(),
            "{:?}",
            self.outcome.report.skipped
        );
    }
}

fn run(host: &mut OffscreenHost, world: &mut World, drawn: &[RenderTextureId]) -> Run {
    let requests: Vec<_> = drawn
        .iter()
        .map(|target| {
            world
                .request_texture_pixels(*target, 0, 0, SIDE, SIDE)
                .unwrap()
        })
        .collect();
    let outcome = flush(host, world);
    let targets = requests
        .into_iter()
        .map(|request| {
            world
                .take_texture_pixels(request)
                .unwrap()
                .chunks_exact(4)
                .map(|texel| std::array::from_fn(|channel| byte(texel[channel])))
                .collect()
        })
        .collect();
    Run { targets, outcome }
}

fn byte(channel: f32) -> u8 {
    let rounded = (channel * 255.0).round();
    (0..=u8::MAX)
        .find(|value| f32::from(*value).to_bits() == rounded.to_bits())
        .expect("readback channel must represent an RGBA8 value")
}

/// One world with one cleared target, drawn by `script`, flushed once.
fn single(script: impl FnOnce(&mut World, MaterialId)) -> Run {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    let material = red(&mut world, "procedural");
    bind_and_clear(&mut world, drawn);
    script(&mut world, material);
    run(&mut host, &mut world, &[drawn])
}

const TRIANGLES: (DrawTopology, Mat4) = (DrawTopology::TriangleList, Mat4::IDENTITY);

#[test]
fn the_vertex_count_decides_which_generated_triangles_draw() {
    let three = single(|world, red| draw(world, (red, 0), TRIANGLES, (3, 1)));
    three.clean();
    // One triangle of the left rectangle: its lower-left half, so the corners of column 1 differ.
    assert_eq!(three.at(0, (0, 0)), RED);
    assert_eq!(three.at(0, (1, 3)), GREEN);
    assert_eq!([three.column(0, 2), three.column(0, 3)], [GREEN, GREEN]);
    let six = single(|world, red| draw(world, (red, 0), TRIANGLES, (6, 1)));
    six.clean();
    assert_eq!(six.columns(0), [RED, RED, GREEN, GREEN]);
    let twelve = single(|world, red| draw(world, (red, 0), TRIANGLES, (12, 1)));
    twelve.clean();
    assert_eq!(twelve.columns(0), [RED; 4]);
    assert_eq!(twelve.outcome.report.built_by_kind["DrawProcedural"], 1);
}

#[test]
fn the_instance_count_repeats_the_draw_at_each_instance_index() {
    let one = single(|world, red| draw(world, (red, 0), TRIANGLES, (6, 1)));
    let two = single(|world, red| draw(world, (red, 0), TRIANGLES, (6, 2)));
    one.clean();
    two.clean();
    assert_eq!(one.columns(0), [RED, RED, GREEN, GREEN]);
    assert_eq!(two.columns(0), [RED; 4]);
}

#[test]
fn zero_vertices_or_zero_instances_draw_nothing_and_fail_nothing() {
    let moved = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
    let narrowed = |world: &mut World| view(world, Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0)));
    // The same script without the empty draw: the frame records and pixels to match.
    let control = single(|world, red| {
        narrowed(world);
        draw(world, (red, 0), (DrawTopology::TriangleList, moved), (6, 1));
    });
    control.clean();
    for counts in [(0, 1), (6, 0)] {
        // An empty draw between a view change and a drawn one: the drawn one still sees the
        // view, and the empty one adds no record to the frame.
        let empty = single(|world, red| {
            narrowed(world);
            draw(world, (red, 0), TRIANGLES, counts);
            draw(world, (red, 0), (DrawTopology::TriangleList, moved), (6, 1));
        });
        empty.clean();
        assert_eq!(empty.columns(0), [GREEN, GREEN, RED, GREEN], "{counts:?}");
        assert_eq!(
            empty.outcome.report.built, control.outcome.report.built,
            "{counts:?}: the empty draw builds no frame record"
        );
        assert_eq!(
            empty.outcome.report.built_by_kind.get("DrawProcedural"),
            Some(&2),
            "{counts:?}: both draws were read"
        );
    }
}

#[test]
fn the_object_matrix_and_the_view_projection_apply_in_order() {
    // Moved right by one, the left rectangle covers the right half.
    let moved = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
    let right = single(|world, red| {
        draw(world, (red, 0), (DrawTopology::TriangleList, moved), (6, 1));
    });
    right.clean();
    assert_eq!(right.columns(0), [GREEN, GREEN, RED, RED]);
    // Halving x after the move puts it over [0, 0.5], column 2 only. Halving first and then
    // moving would cover [0.5, 1], column 3, so the order shows.
    let narrowed = single(|world, red| {
        view(world, Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0)));
        draw(world, (red, 0), (DrawTopology::TriangleList, moved), (6, 1));
    });
    narrowed.clean();
    assert_eq!(narrowed.columns(0), [GREEN, GREEN, RED, GREEN]);
}

#[test]
fn a_line_list_draws_lines_and_a_triangle_list_with_the_same_material_still_draws_triangles() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let red = red(&mut world, "procedural");
    let panels = [0, 1, 2].map(|_| target(&mut world, ARGB32));
    // Vertices 0 and 1 run along y = -1 from x = -1 to x = 0; moved up by 1.25 they run through
    // the centres of row 2 of columns 0 and 1.
    let lifted = Mat4::from_translation(Vec3::new(0.0, 1.25, 0.0));
    for (panel, topology) in panels.iter().zip([
        DrawTopology::TriangleList,
        DrawTopology::LineList,
        DrawTopology::TriangleList,
    ]) {
        bind_and_clear(&mut world, *panel);
        let (matrix, count) = match topology {
            DrawTopology::LineList => (lifted, 2),
            DrawTopology::TriangleList => (Mat4::IDENTITY, 6),
        };
        draw(&mut world, (red, 0), (topology, matrix), (count, 1));
    }
    let run = run(&mut host, &mut world, &panels);
    run.clean();
    assert_eq!(run.columns(0), [RED, RED, GREEN, GREEN]);
    assert_eq!(run.columns(2), [RED, RED, GREEN, GREEN]);
    for column in 0..SIDE {
        for row in 0..SIDE {
            let expected = if row == 2 && column < 2 { RED } else { GREEN };
            assert_eq!(
                run.at(1, (column, row)),
                expected,
                "line panel column {column} row {row}"
            );
        }
    }
}

#[test]
fn each_draw_reads_the_global_array_in_force_at_its_place() {
    let set = |world: &mut World, color: [f32; 4]| {
        record(
            world,
            OffscreenWork::SetGlobal(PropertyValue::VectorArray(
                "_Colors".into(),
                vec![[0.0; 4], color],
            )),
        );
    };
    let run = single(|world, material| {
        set(world, [1.0, 0.0, 0.0, 1.0]);
        draw(world, (material, 1), TRIANGLES, (6, 1));
        set(world, [0.0, 0.0, 1.0, 1.0]);
        let moved = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        draw(
            world,
            (material, 1),
            (DrawTopology::TriangleList, moved),
            (6, 1),
        );
        // A change after both draws reaches neither.
        set(world, [1.0, 1.0, 1.0, 1.0]);
    });
    run.clean();
    assert_eq!(run.columns(0), [RED, RED, BLUE, BLUE]);
}

#[test]
fn pass_minus_one_draws_every_pass_in_order() {
    // Pass 0 writes the red `_Tint`, pass 1 the blue `_Colors[1]`; drawn in order over the left
    // rectangle, pass 1 is drawn last. Pass 0 alone over the right rectangle stays red.
    let run = single(|world, material| {
        record(
            world,
            OffscreenWork::SetGlobal(PropertyValue::VectorArray(
                "_Colors".into(),
                vec![[0.0; 4], [0.0, 0.0, 1.0, 1.0]],
            )),
        );
        draw(world, (material, -1), TRIANGLES, (6, 1));
        let moved = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        draw(
            world,
            (material, 0),
            (DrawTopology::TriangleList, moved),
            (6, 1),
        );
    });
    run.clean();
    assert_eq!(run.columns(0), [BLUE, BLUE, RED, RED]);
}

#[test]
fn a_pass_whose_vertex_stage_reads_vertex_inputs_is_refused_by_name() {
    // The pass also samples a texture nothing names; the vertex inputs are the cause reported.
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    let refused = red(&mut world, "procedural_inputs");
    let drawing = red(&mut world, "procedural");
    bind_and_clear(&mut world, drawn);
    draw(&mut world, (refused, 0), TRIANGLES, (6, 1));
    let moved = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
    draw(
        &mut world,
        (drawing, 0),
        (DrawTopology::TriangleList, moved),
        (6, 1),
    );
    let run = run(&mut host, &mut world, &[drawn]);
    let failed = failures(&run.outcome);
    assert!(
        failed
            .iter()
            .any(|(name, _)| name.contains("ProceduralVertexInputs")),
        "{failed:?}"
    );
    // The refused draw leaves its half clear; the draw after it still draws.
    assert_eq!(run.columns(0), [GREEN, GREEN, RED, RED]);
}
