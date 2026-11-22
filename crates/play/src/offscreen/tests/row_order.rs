//! Which rows of a render texture a scene's top lands in, through a stage that negates clip Y as
//! every compiled player stage does, on a real adapter (lavapipe in CI).
//!
//! A render texture holds its rows bottom first, as a texture the player stored does, so a shader
//! that samples both with one texture coordinate finds the same part of the picture in either:
//! the top of what a camera drew is in the last rows, and the world counts the rows of a read the
//! same way. Old World bakes the height of each terrain tile with a camera into a render texture
//! and displaces the tile's vertices by that texture at their own coordinate, so a bake whose rows
//! came out top first put the height of the tile's far edge on its near edge, and the edges of
//! neighbouring tiles no longer met.
//!
//! The shader is synthetic: the data of the player's shaders is not public, so the stage
//! reproduces the one property that matters, a position output with Y negated. It places a vertex
//! by `unity_ObjectToWorld` and `unity_MatrixVP`, writes the material's `_Tint`, and keeps the
//! default `Cull Back` of a pass with no `Cull` line. The quad is drawn twice in the same place,
//! once wound for a viewer in front and once reversed, so exactly one of them survives the cull
//! whichever way a stage's winding comes out, and what is measured is the rows alone.

use glam::{Mat4, Vec3};
use worldrunner_disunity_runtime::{
    MeshChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, VERTEX, binding, constant_buffer, matrix_member,
    shader as synthetic_shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, ShaderSource, TargetRef,
};
use worldrunner_world_simulation::presentation::{Camera, Projection};

use super::super::adapter::OffscreenAdapter;
use super::super::assets::{AssetError, Assets};
use super::super::camera::projection;
use super::super::compute::ComputeAdapter;
use super::super::defaults::{ShaderFill, declared};
use super::super::flush::OffscreenHost;
use super::super::frame::Settings;
use super::super::kernels::PlayerKernels;
use super::super::shaders::{LoadedShader, ShaderLoadError, ShaderSupply};
use super::gpu::{SIDE, gpu, shader, target, vector};
use super::support::{snapshot, world};

const ARGB32: u32 = 8;
/// The quad wound clockwise for a viewer on the negative z side, and the same quad reversed.
const FRONT: u64 = (1 << 41) + 11;
const BACK: u64 = (1 << 41) + 12;
const CLEARED: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const DRAWN: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

const WGSL: &str = "
struct Material { tint: vec4<f32> }
struct Draw { object_to_world: mat4x4<f32>, view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    var clip = draw.view_projection * (draw.object_to_world * vec4<f32>(position, 1.0));
    clip.y = -clip.y;
    out.position = clip;
    out.uv = uv;
    return out;
}
@fragment fn fs_tint(in: VOut) -> @location(0) vec4<f32> { return material.tint; }";

fn parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![
            constant_buffer("Material", 16, vec![vector_member("_Tint", 0)]),
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

struct Shaders;

impl ShaderSupply for Shaders {
    fn load(&mut self, _: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        let mut spec = variant(&[], WGSL, "fs_tint", parameters());
        spec.channels = POSITION_AND_UV.to_vec();
        let pass = PassSpec {
            tags: Vec::new(),
            variants: vec![spec],
        };
        let synthetic = synthetic_shader(&[], &[pass]);
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

/// The player's quad: four vertices in the plane z = 0 over -0.5 to 0.5.
struct Quads;

impl Assets for Quads {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        if known == 1 || (identity != FRONT && identity != BACK) {
            return Ok(None);
        }
        let indices = if identity == FRONT {
            vec![0, 3, 1, 3, 0, 2]
        } else {
            vec![0, 1, 3, 3, 2, 0]
        };
        let positions = [
            -0.5, -0.5, 0.0, 0.5, -0.5, 0.0, -0.5, 0.5, 0.0, 0.5, 0.5, 0.0,
        ];
        let coordinates = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        Ok(Some(MeshChannels {
            generation: 1,
            vertex_count: 4,
            channels: vec![
                MeshChannel {
                    number: 0,
                    dimension: 3,
                    data: positions.to_vec(),
                },
                MeshChannel {
                    number: 4,
                    dimension: 2,
                    data: coordinates.to_vec(),
                },
            ],
            indices,
            ranges: vec![MeshRange {
                start: 0,
                count: 6,
                base_vertex: 0,
                topology: 0,
            }],
        }))
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

fn host(directory: &std::path::Path) -> OffscreenHost {
    let gpu = gpu();
    let settings = Settings {
        ..Settings::default()
    };
    OffscreenHost::new(
        gpu.runtime,
        gpu.arena,
        OffscreenAdapter::with_supply(Shaders, settings),
        ComputeAdapter::new(PlayerKernels::new(directory)),
    )
}

/// The view the engine layer gives a camera at -2 on z looking along +z: the inverse of its pose
/// with the depth axis negated, as OpenGL's camera space looks down negative Z.
fn view() -> Mat4 {
    Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
        * Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0)).inverse()
}

/// The projection of a square view a quarter turn wide, as a camera records it.
fn lens() -> Mat4 {
    let settings = Camera {
        projection: Projection::Perspective,
        field_of_view: 90.0,
        near: 0.3,
        far: 100.0,
        ..Camera::default()
    };
    projection(&settings, 1.0)
}

/// Clears `drawn` to [`CLEARED`], sets the view and projection of a command buffer and draws both
/// windings of a quad that covers the upper half of the view: at the distance of 2 the view spans
/// -2 to 2 on each axis, and the quad spans -2 to 2 across and 0 to 2 up.
fn record_upper_half(world: &mut World, drawn: RenderTextureId) {
    let shader = shader(world, "stage");
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", DRAWN);
    let matrix = (Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(4.0, 2.0, 1.0)))
    .to_cols_array();
    let draw = |mesh| OffscreenWork::DrawMesh {
        mesh,
        matrix,
        material,
        submesh: 0,
        pass: 0,
        block: None,
        instances: 1,
    };
    let commands = [
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: CLEARED,
            depth_value: 1.0,
            stencil_value: 0,
        },
        OffscreenWork::SetViewProjection {
            view: view().to_cols_array(),
            projection: lens().to_cols_array(),
        },
        draw(FRONT),
        draw(BACK),
    ];
    for command in commands {
        world.record_offscreen(command).unwrap();
    }
}

/// Every texel of the target after one flush, rows bottom first.
fn read(mut world: World, drawn: RenderTextureId) -> Vec<[f32; 4]> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quads).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    world
        .take_texture_pixels(request)
        .unwrap()
        .chunks_exact(4)
        .map(|texel| [texel[0], texel[1], texel[2], texel[3]])
        .collect()
}

#[test]
fn the_top_of_what_a_command_buffer_draws_is_in_the_last_rows_of_a_render_texture() {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    record_upper_half(&mut world, drawn);

    let texels = read(world, drawn);

    // Rows bottom first: the upper half of the view is rows 2 and 3 of the four.
    let side = SIDE as usize;
    let expected: Vec<[f32; 4]> = (0..side * side)
        .map(|index| {
            if index / side >= side / 2 {
                DRAWN
            } else {
                CLEARED
            }
        })
        .collect();
    assert_eq!(texels, expected);
}
