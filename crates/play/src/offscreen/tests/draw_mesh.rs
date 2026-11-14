//! A command buffer's mesh draw on a real adapter (lavapipe in CI): a render target is bound and
//! cleared, a view and projection are set, and `DrawMesh` records draw one sub-mesh of a stored
//! mesh with a matrix and a material's passes. A pixel read sees the texels.
//!
//! The shaders and meshes are synthetic. The shader places a vertex by the draw's
//! `unity_ObjectToWorld` and `unity_MatrixVP` and writes a constant of its material, so every
//! expected texel comes from the matrices, the colours and the blend factors, not from the output.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};
use worldrunner_disunity_runtime::{
    MeshChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, binding, constant_buffer, fixed, matrix_member,
    shader as synthetic_shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RenderTextureId, ShaderSource, TargetRef,
};

use super::super::adapter::OffscreenAdapter;
use super::super::assets::{AssetError, Assets};
use super::super::compute::ComputeAdapter;
use super::super::defaults::{ShaderFill, declared};
use super::super::flush::{FlushOutcome, OffscreenHost};
use super::super::frame::{Settings, Skip};
use super::super::kernels::PlayerKernels;
use super::super::shaders::{LoadedShader, ShaderLoadError, ShaderSupply};
use super::gpu::{Gpu, SIDE, gpu, gpu_with_budget, shader, target, vector};
use super::support::{snapshot, world};

const ARGB32: u32 = 8;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const MESH: u64 = (1 << 41) + 5;
const OTHER_MESH: u64 = (1 << 41) + 6;

mod block;
mod persistent;

/// A vertex stage that places its position by the draw's matrices, a fragment stage that writes
/// the material's `_Tint` and one that writes its `_Over`.
const WGSL: &str = "
struct Material { tint: vec4<f32>, over: vec4<f32> }
struct Draw { object_to_world: mat4x4<f32>, view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = draw.view_projection * (draw.object_to_world * vec4<f32>(position, 1.0));
    out.uv = uv;
    return out;
}
@fragment fn fs_tint(in: VOut) -> @location(0) vec4<f32> { return material.tint; }
@fragment fn fs_over(in: VOut) -> @location(0) vec4<f32> { return material.over; }";

fn parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![
            constant_buffer(
                "Material",
                32,
                vec![vector_member("_Tint", 0), vector_member("_Over", 16)],
            ),
            constant_buffer(
                "Draw",
                128,
                vec![
                    matrix_member("unity_ObjectToWorld", 0),
                    matrix_member("unity_MatrixVP", 64),
                ],
            ),
        ],
        bindings: vec![
            binding(
                "Material",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
        ],
    }
}

fn pass(fragment: &'static str) -> PassSpec {
    let mut spec = variant(&[], WGSL, fragment, parameters());
    spec.channels = POSITION_AND_UV.to_vec();
    PassSpec {
        tags: Vec::new(),
        variants: vec![spec],
    }
}

/// `flat`: one pass that writes `_Tint`. `layers`: pass 0 writes `_Tint` as it is, and pass 1
/// writes `_Over` blended by its alpha over what the target holds (source alpha, one minus
/// source alpha).
fn synthetic(file: &str) -> SyntheticShader {
    match file {
        "flat" => no_cull(synthetic_shader(&[], &[pass("fs_tint")])),
        "layers" => {
            let mut layers = no_cull(synthetic_shader(&[], &[pass("fs_tint"), pass("fs_over")]));
            let blend = &mut layers.shader.parsed.sub_shaders[0].passes[1].state.blend[0];
            blend.src = fixed(5.0);
            blend.dst = fixed(10.0);
            layers
        }
        "sampled" => block::sampled_passes(),
        other => panic!("no synthetic shader {other}"),
    }
}

struct Shaders;

impl ShaderSupply for Shaders {
    fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        let synthetic = synthetic(source.file.as_str());
        let texture_defaults = declared(&synthetic.shader);
        let program = ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap();
        Ok(LoadedShader {
            passes: program.pass_count(),
            program,
            queue: 2000,
            texture_defaults,
            declared_defaults: Vec::new(),
            fill: ShaderFill::default(),
        })
    }
}

/// The meshes the host stores, by identity.
struct Meshes(BTreeMap<u64, MeshChannels>);

impl Assets for Meshes {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Ok(self
            .0
            .get(&identity)
            .filter(|mesh| mesh.generation != known)
            .cloned())
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// A rectangle's extent along x and along y, each as its lowest and highest value.
#[derive(Clone, Copy)]
struct Rectangle {
    x: (f32, f32),
    y: (f32, f32),
}

/// The vertices of a rectangle `[x0, x1] x [y0, y1]` at depth 0, with texture coordinates.
fn vertices(
    Rectangle {
        x: (x0, x1),
        y: (y0, y1),
    }: Rectangle,
) -> (Vec<f32>, Vec<f32>) {
    (
        vec![x0, y0, 0.0, x1, y0, 0.0, x0, y1, 0.0, x1, y1, 0.0],
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    )
}

/// A mesh of one sub-mesh for each rectangle, in order: two triangles over four vertices.
fn mesh_of(rectangles: &[Rectangle]) -> MeshChannels {
    let (mut positions, mut coordinates) = (Vec::new(), Vec::new());
    let (mut indices, mut ranges) = (Vec::new(), Vec::new());
    for (number, rectangle) in (0_u32..).zip(rectangles) {
        let (rectangle_positions, rectangle_coordinates) = vertices(*rectangle);
        positions.extend(rectangle_positions);
        coordinates.extend(rectangle_coordinates);
        let first = number * 4;
        ranges.push(MeshRange {
            start: number * 6,
            count: 6,
            base_vertex: 0,
            topology: 0,
        });
        indices.extend([first, first + 1, first + 2, first + 2, first + 1, first + 3]);
    }
    MeshChannels {
        generation: 1,
        vertex_count: u32::try_from(rectangles.len()).unwrap() * 4,
        channels: vec![
            MeshChannel {
                number: 0,
                dimension: 3,
                data: positions,
            },
            MeshChannel {
                number: 4,
                dimension: 2,
                data: coordinates,
            },
        ],
        indices,
        ranges,
    }
}

const LEFT: Rectangle = Rectangle {
    x: (-1.0, 0.0),
    y: (-1.0, 1.0),
};
const RIGHT_QUARTER: Rectangle = Rectangle {
    x: (0.5, 1.0),
    y: (-1.0, 1.0),
};

/// The left half of clip space under an identity view and projection.
fn left_half() -> MeshChannels {
    mesh_of(&[LEFT])
}

fn stored(meshes: impl IntoIterator<Item = (u64, MeshChannels)>) -> Meshes {
    Meshes(meshes.into_iter().collect())
}

/// A world with a render target bound and cleared to green.
fn scene() -> (World, RenderTextureId) {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    bind_and_clear(&mut world, drawn);
    (world, drawn)
}

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
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

fn set_view_projection(world: &mut World, view: Mat4, projection: Mat4) {
    record(
        world,
        OffscreenWork::SetViewProjection {
            view: view.to_cols_array(),
            projection: projection.to_cols_array(),
        },
    );
}

/// The OpenGL projection of the box 0 to 1 in x and y and -1 to 1 in z: x and y become 2 v - 1 and
/// z is negated.
fn orthographic_unit_box() -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, -1.0, 0.0)) * Mat4::from_scale(Vec3::new(2.0, 2.0, -1.0))
}

fn identity_view(world: &mut World) {
    set_view_projection(world, Mat4::IDENTITY, Mat4::IDENTITY);
}

/// A material of the shader in `file` with `_Tint` and `_Over` of the colours given.
fn material(world: &mut World, file: &str, tint: [f32; 4], over: [f32; 4]) -> MaterialId {
    let shader = shader(world, file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", tint);
    vector(world, material, "_Over", over);
    material
}

fn flat(world: &mut World, tint: [f32; 4]) -> MaterialId {
    material(world, "flat", tint, [0.0; 4])
}

fn draw_mesh(
    world: &mut World,
    (mesh, matrix): (u64, Mat4),
    material: MaterialId,
    (submesh, pass): (u32, i32),
) {
    record(
        world,
        OffscreenWork::DrawMesh {
            mesh,
            matrix: matrix.to_cols_array(),
            material,
            submesh,
            pass,
            block: None,
        },
    );
}

/// What a flush of the world's work left in the target, rows bottom first, as bytes.
struct Run {
    texels: Vec<[u8; 4]>,
    outcome: FlushOutcome,
}

impl Run {
    fn at(&self, column: u32, row: u32) -> [u8; 4] {
        self.texels[(row * SIDE + column) as usize]
    }

    /// The texel of every row of `column`, which must agree.
    fn column(&self, column: u32) -> [u8; 4] {
        let first = self.at(column, 0);
        for row in 0..SIDE {
            assert_eq!(self.at(column, row), first, "column {column} row {row}");
        }
        first
    }

    fn columns(&self) -> Vec<[u8; 4]> {
        (0..SIDE).map(|column| self.column(column)).collect()
    }
}

fn host(kernels: PlayerKernels) -> OffscreenHost {
    host_over(gpu(), kernels)
}

/// A host whose runtime may hold `budget` bytes of storage.
fn host_with_budget(kernels: PlayerKernels, budget: u64) -> OffscreenHost {
    host_over(gpu_with_budget(budget), kernels)
}

fn host_over(gpu: Gpu, kernels: PlayerKernels) -> OffscreenHost {
    OffscreenHost::new(
        gpu.runtime,
        gpu.arena,
        OffscreenAdapter::with_supply(
            Shaders,
            Settings {
                light_modes: vec!["Alpha".to_owned()],
                ..Settings::default()
            },
        ),
        ComputeAdapter::new(kernels),
    )
}

fn flush(world: &mut World, drawn: RenderTextureId, meshes: &Meshes) -> Run {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(PlayerKernels::new(directory.path()));
    flush_in(&mut host, world, drawn, meshes)
}

fn flush_in(
    host: &mut OffscreenHost,
    world: &mut World,
    drawn: RenderTextureId,
    meshes: &Meshes,
) -> Run {
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, meshes).unwrap();
    outcome.apply(world).unwrap();
    let read = world.take_texture_pixels(request).unwrap();
    let texels = read
        .chunks_exact(4)
        .map(|texel| std::array::from_fn(|channel| byte(texel[channel])))
        .collect();
    Run { texels, outcome }
}

/// The 8-bit value the target held for a channel that came back as a float.
fn byte(channel: f32) -> u8 {
    let rounded = (channel * 255.0).round();
    (0..=u8::MAX)
        .find(|value| f32::from(*value).to_bits() == rounded.to_bits())
        .expect("readback channel must represent an RGBA8 value")
}

#[test]
fn a_mesh_over_the_left_half_of_clip_space_draws_the_left_half_of_the_target() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [RED, RED, GREEN, GREEN]);
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert!(run.outcome.report.skipped.is_empty());
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 1);
}

#[test]
fn the_object_matrix_moves_the_mesh() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    // One clip unit to the right takes [-1, 0] to [0, 1].
    let right = Mat4::from_translation(Vec3::X);
    draw_mesh(&mut world, (MESH, right), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [GREEN, GREEN, RED, RED]);
}

#[test]
fn a_view_and_projection_set_by_the_script_map_the_mesh_as_predicted() {
    let (mut world, drawn) = scene();
    // Identity view and an orthographic projection of the box 0 to 1 on both axes, as a portrait
    // pass sets: x and y of 0 to 1 become clip -1 to 1.
    set_view_projection(&mut world, Mat4::IDENTITY, orthographic_unit_box());
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    // The mesh covers -1 to 0 in x and -1 to 1 in y. Scaled by 0.5 in x and 0.25 in y and moved
    // by (0.75, 0.25), it covers x 0.25 to 0.75 and y 0 to 0.5 of the box: clip x -0.5 to 0.5,
    // the columns 1 and 2, and clip y -1 to 0, the rows 0 and 1.
    let matrix = Mat4::from_translation(Vec3::new(0.75, 0.25, 0.0))
        * Mat4::from_scale(Vec3::new(0.5, 0.25, 1.0));
    draw_mesh(&mut world, (MESH, matrix), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    let expected = |column: u32, row: u32| {
        if (1..=2).contains(&column) && row < 2 {
            RED
        } else {
            GREEN
        }
    };
    for row in 0..SIDE {
        for column in 0..SIDE {
            assert_eq!(
                run.at(column, row),
                expected(column, row),
                "column {column} row {row}"
            );
        }
    }
}

#[test]
fn a_later_view_projection_replaces_the_earlier_one_for_the_draws_after_it() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    // A projection that halves x takes the mesh's -1 to 0 to clip -0.5 to 0, which is column 1.
    set_view_projection(
        &mut world,
        Mat4::IDENTITY,
        Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0)),
    );
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), blue, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [RED, BLUE, GREEN, GREEN]);
}

#[test]
fn sub_mesh_1_draws_only_its_own_triangles() {
    // Sub-mesh 0 covers the left half and sub-mesh 1 the right quarter.
    let two = mesh_of(&[LEFT, RIGHT_QUARTER]);
    for (submesh, expected) in [
        (1, [GREEN, GREEN, GREEN, RED]),
        (0, [RED, RED, GREEN, GREEN]),
    ] {
        let (mut world, drawn) = scene();
        identity_view(&mut world);
        let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
        draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (submesh, -1));

        let run = flush(&mut world, drawn, &stored([(MESH, two.clone())]));

        assert_eq!(run.columns(), expected, "sub-mesh {submesh}");
    }
}

#[test]
fn pass_minus_1_draws_the_passes_in_order_and_the_second_blends_over_the_first() {
    let tint = [1.0, 0.0, 0.0, 1.0];
    // Alpha 0.4 is 102 of 255, and the blend is 0.4 of blue over 0.6 of what is there.
    let over = [0.0, 0.0, 1.0, 0.4];
    let meshes = stored([(MESH, left_half())]);
    // Every pass: red written first, then 0.4 of blue over it: red 0.6, blue 0.4, alpha 0.4.
    // Drawn the other way round the opaque red would be all that is left.
    for (pass, expected) in [
        (-1, [[153, 0, 102, 102], [153, 0, 102, 102], GREEN, GREEN]),
        (0, [RED, RED, GREEN, GREEN]),
        // Pass 1 alone blends over the green the target was cleared to.
        (1, [[0, 153, 102, 102], [0, 153, 102, 102], GREEN, GREEN]),
    ] {
        let (mut world, drawn) = scene();
        identity_view(&mut world);
        let layers = material(&mut world, "layers", tint, over);
        draw_mesh(&mut world, (MESH, Mat4::IDENTITY), layers, (0, pass));

        let run = flush(&mut world, drawn, &meshes);

        assert_eq!(run.columns(), expected, "pass {pass}");
    }
}

#[test]
fn a_mesh_the_store_does_not_hold_is_refused_as_unavailable() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (OTHER_MESH, Mat4::IDENTITY), red, (0, -1));
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.outcome.report.skipped[&Skip::MeshUnavailable], 1);
    assert_eq!(
        run.outcome.report.skipped_by_kind["DrawMesh: MeshUnavailable"],
        1
    );
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 1);
    assert_eq!(run.columns(), [RED, RED, GREEN, GREEN]);
}

#[test]
fn a_sub_mesh_past_the_meshs_count_is_refused() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    // The mesh has one sub-mesh, numbered 0.
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (1, -1));
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (7, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.outcome.report.skipped[&Skip::NoSuchSubmesh], 2);
    assert_eq!(
        run.outcome.report.skipped_by_kind["DrawMesh: NoSuchSubmesh"],
        2
    );
    assert_eq!(run.outcome.report.built_by_kind.get("DrawMesh"), None);
    assert_eq!(run.columns(), [GREEN; 4]);
}

#[test]
fn a_pass_past_the_shaders_passes_is_refused() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    // The flat shader has one pass, numbered 0.
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, 1));
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, 40));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.outcome.report.skipped[&Skip::NoSuchPass], 2);
    assert_eq!(
        run.outcome.report.skipped_by_kind["DrawMesh: NoSuchPass"],
        2
    );
    assert_eq!(run.columns(), [GREEN; 4]);
}

#[test]
fn an_empty_mesh_is_refused_and_counted_like_any_empty_mesh() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    let mut empty = left_half();
    empty.indices.clear();
    empty.ranges.clear();

    let run = flush(&mut world, drawn, &stored([(MESH, empty)]));

    let report = &run.outcome.report;
    assert_eq!(report.skipped[&Skip::EmptyMesh], 1);
    assert_eq!(report.skipped_by_kind["DrawMesh: EmptyMesh"], 1);
    assert_eq!(
        report.census["mesh with nothing to draw: 0 sub-meshes with topologies [], no indices, with vertices"],
        1
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(run.columns(), [GREEN; 4]);
}

#[test]
fn a_sub_mesh_with_no_indices_is_refused_as_empty() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (1, -1));
    // Sub-mesh 0 draws, and sub-mesh 1 is a range of no indices.
    let mut gap = left_half();
    gap.ranges.push(MeshRange {
        start: 6,
        count: 0,
        base_vertex: 0,
        topology: 0,
    });

    let run = flush(&mut world, drawn, &stored([(MESH, gap)]));

    assert_eq!(run.outcome.report.skipped[&Skip::EmptyMesh], 1);
    assert_eq!(run.columns(), [GREEN; 4]);
}

#[test]
fn a_mesh_draw_with_no_render_target_bound_is_refused() {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.outcome.report.skipped[&Skip::UnboundTarget], 1);
}
