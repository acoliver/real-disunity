//! Which triangles a back-culling pass keeps, on a real adapter (lavapipe in CI): a quad wound
//! the way the player's own quad is, clockwise for a viewer in front of it, drawn by a stage
//! that negates clip Y as every compiled player stage does, under the upright projection a
//! camera records. A viewer in front of the quad sees its front face, so the texture must hold
//! the quad's colour, and the same quad with its indices reversed shows its back and is culled.
//!
//! The three ways a pass draws, a renderer list, a command buffer's mesh draw and its
//! procedural draw, share the host's front-face choice, and each is read here.
//!
//! The shaders are synthetic: the data of the player's shaders is not public, so the stage
//! reproduces the one property that matters, a position output with Y negated. They place a vertex
//! by `unity_ObjectToWorld` and `unity_MatrixVP`, write the material's `_Tint`, and keep the
//! default `Cull Back` of a pass with no `Cull` line.

use glam::{Mat4, Quat, Vec3};
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
use worldrunner_world_simulation::graphics::{
    DrawTopology, OffscreenWork, RenderTextureId, RendererListDesc, ShaderSource, TargetRef,
};
use worldrunner_world_simulation::presentation::{Camera, Projection};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc, World,
};

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
use super::support::{camera, snapshot, world};

const ARGB32: u32 = 8;
const MESH: u64 = (1 << 41) + 9;
const CLEARED: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const DRAWN: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// A vertex stage that places its position by the draw's matrices and negates clip Y, a fragment
/// stage that writes the material's `_Tint`. `ORDER` is the six corners of a generated quad.
const WGSL: &str = "
struct Material { tint: vec4<f32> }
struct Draw { object_to_world: mat4x4<f32>, view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
fn place(p: vec3<f32>) -> vec4<f32> {
    var clip = draw.view_projection * (draw.object_to_world * vec4<f32>(p, 1.0));
    clip.y = -clip.y;
    return clip;
}
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = place(position);
    out.uv = uv;
    return out;
}
@vertex fn vs_generated(@builtin(vertex_index) v: u32) -> VOut {
    var corners = array<vec2<f32>, 6>(ORDER);
    var out: VOut;
    out.position = place(vec3<f32>(corners[v % 6u], 0.0));
    out.uv = vec2<f32>(0.0, 0.0);
    return out;
}
@fragment fn fs_tint(in: VOut) -> @location(0) vec4<f32> { return material.tint; }";

/// The corners of the player's quad in the order that is its front face for a viewer on the
/// negative z side, and the reverse order.
const FRONT_ORDER: &str = "vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(0.5, -0.5), \
     vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, -0.5), vec2<f32>(-0.5, 0.5)";
const BACK_ORDER: &str = "vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5), \
     vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5), vec2<f32>(-0.5, -0.5)";

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

/// The one pass of the shader in `file`: `stage` reads a position and a texture coordinate and
/// `stage_alpha` is the same pass tagged `Alpha` for a camera render, `generated_front` and
/// `generated_back` read no vertex input and generate the quad's corners in the front or the
/// reverse order.
fn pass(file: &str) -> PassSpec {
    let (generated, order) = match file {
        "stage" | "stage_alpha" => (false, FRONT_ORDER),
        "generated_front" => (true, FRONT_ORDER),
        "generated_back" => (true, BACK_ORDER),
        other => panic!("no synthetic shader {other}"),
    };
    let mut wgsl = WGSL.replace("ORDER", order);
    if generated {
        wgsl = wgsl
            .replace("vs_main", "vs_unused")
            .replace("vs_generated", "vs_main");
    }
    let mut spec = variant(&[], &wgsl, "fs_tint", parameters());
    spec.channels = if generated {
        Vec::new()
    } else {
        POSITION_AND_UV.to_vec()
    };
    PassSpec {
        tags: if file == "stage_alpha" {
            vec![("LIGHTMODE", "Alpha")]
        } else {
            Vec::new()
        },
        variants: vec![spec],
    }
}

struct Shaders;

impl ShaderSupply for Shaders {
    fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        let synthetic = synthetic_shader(&[], &[pass(&source.file)]);
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

/// The player's quad: four vertices in the plane z = 0 over -0.5 to 0.5. Its triangles are wound
/// clockwise for a viewer on the negative z side, or the other way round when `reversed`.
struct Quad {
    reversed: bool,
}

impl Assets for Quad {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        if identity != MESH || known == 1 {
            return Ok(None);
        }
        let indices = if self.reversed {
            vec![0, 1, 3, 3, 2, 0]
        } else {
            vec![0, 3, 1, 3, 0, 2]
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
        light_modes: vec!["Alpha".to_owned()],
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

/// The upright projection of a square view a quarter turn wide, as a camera records it.
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

/// A quad of `size` at the origin facing the camera, drawn by a renderer with the shader `file`.
fn renderer_quad(world: &mut World, file: &str, size: f32) {
    let shader = shader(world, file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", DRAWN);
    let object = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::new(size, size, 1.0),
            },
            ..ObjectDesc::default()
        })
        .unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh: MESH,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        ..MeshRenderer::default()
    };
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
}

/// Binds `drawn` and clears it, then records a camera's setup and its draw of every renderer.
fn pipeline(world: &mut World, drawn: RenderTextureId) {
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let commands = [
        bind(drawn),
        clear(),
        OffscreenWork::SetupCamera {
            camera,
            view: view().to_cols_array(),
            projection: lens().to_cols_array(),
            target: TargetRef::Render(drawn),
            buffers: None,
        },
        OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        },
    ];
    for command in commands {
        world.record_offscreen(command).unwrap();
    }
    let list = world
        .create_renderer_list(RendererListDesc::Draw {
            cull: 5,
            pass_tags: vec!["SRPDefaultUnlit".to_owned()],
            queue_min: 0,
            queue_max: 5000,
            layer_mask: u32::MAX,
            rendering_layer_mask: u32::MAX,
            sorting: 0,
            per_object: 0,
            override_material: None,
            override_pass: 0,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
}

fn bind(drawn: RenderTextureId) -> OffscreenWork {
    OffscreenWork::SetRenderTarget {
        color: vec![TargetRef::Render(drawn)],
        depth: None,
        mip: 0,
        face: -1,
        slice: -1,
    }
}

fn clear() -> OffscreenWork {
    OffscreenWork::ClearRenderTarget {
        color: true,
        depth: false,
        stencil: false,
        value: CLEARED,
        depth_value: 1.0,
        stencil_value: 0,
    }
}

/// Binds `drawn`, clears it and sets the view and projection of a command buffer, which then
/// draws with `draw`.
fn command_buffer(world: &mut World, drawn: RenderTextureId, file: &str, draw: DrawKind) {
    let shader = shader(world, file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", DRAWN);
    let matrix = Mat4::from_scale(Vec3::new(4.0, 4.0, 1.0)).to_cols_array();
    let view = OffscreenWork::SetViewProjection {
        view: view().to_cols_array(),
        projection: lens().to_cols_array(),
    };
    let draw = match draw {
        DrawKind::Mesh => OffscreenWork::DrawMesh {
            mesh: MESH,
            matrix,
            material,
            submesh: 0,
            pass: 0,
            block: None,
            instances: 1,
        },
        DrawKind::Procedural => OffscreenWork::DrawProcedural {
            matrix,
            material,
            pass: 0,
            topology: DrawTopology::TriangleList,
            vertex_count: 6,
            instance_count: 1,
        },
    };
    for command in [bind(drawn), clear(), view, draw] {
        world.record_offscreen(command).unwrap();
    }
}

#[derive(Clone, Copy)]
enum DrawKind {
    Mesh,
    Procedural,
}

/// Every texel of the target after one flush, rows bottom first.
fn read(mut world: World, drawn: RenderTextureId, reversed: bool) -> Vec<[f32; 4]> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad { reversed }).unwrap();
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

fn all(texel: [f32; 4]) -> Vec<[f32; 4]> {
    vec![texel; (SIDE * SIDE) as usize]
}

fn list_draw(reversed: bool) -> Vec<[f32; 4]> {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    renderer_quad(&mut world, "stage", 4.0);
    pipeline(&mut world, drawn);
    read(world, drawn, reversed)
}

/// A camera at -2 on z that clears to green and draws every renderer into `drawn`.
fn camera_draw(reversed: bool) -> Vec<[f32; 4]> {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    renderer_quad(&mut world, "stage_alpha", 4.0);
    let settings = Camera {
        projection: Projection::Perspective,
        field_of_view: 90.0,
        near: 0.3,
        far: 100.0,
        background: [0.0, 1.0, 0.0, 1.0],
        ..Camera::default()
    };
    let camera = camera(
        &mut world,
        settings,
        Vec3::new(0.0, 0.0, -2.0),
        Quat::IDENTITY,
    );
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: drawn,
        })
        .unwrap();
    read(world, drawn, reversed)
}

fn buffer_draw(kind: DrawKind, file: &str, reversed: bool) -> Vec<[f32; 4]> {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    command_buffer(&mut world, drawn, file, kind);
    read(world, drawn, reversed)
}

#[test]
fn a_renderer_list_draws_a_back_culled_quad_that_faces_the_camera_through_a_stage_that_negates_clip_y()
 {
    assert_eq!(list_draw(false), all(DRAWN));
}

#[test]
fn a_renderer_list_culls_a_back_culled_quad_whose_indices_are_reversed() {
    assert_eq!(list_draw(true), all(CLEARED));
}

#[test]
fn a_mesh_draw_draws_a_back_culled_quad_that_faces_the_camera_through_a_stage_that_negates_clip_y()
{
    assert_eq!(buffer_draw(DrawKind::Mesh, "stage", false), all(DRAWN));
}

#[test]
fn a_mesh_draw_culls_a_back_culled_quad_whose_indices_are_reversed() {
    assert_eq!(buffer_draw(DrawKind::Mesh, "stage", true), all(CLEARED));
}

#[test]
fn a_procedural_draw_draws_back_culled_corners_in_the_front_order() {
    let found = buffer_draw(DrawKind::Procedural, "generated_front", false);
    assert_eq!(found, all(DRAWN));
}

#[test]
fn a_procedural_draw_culls_back_culled_corners_in_the_reverse_order() {
    let found = buffer_draw(DrawKind::Procedural, "generated_back", false);
    assert_eq!(found, all(CLEARED));
}

#[test]
fn a_camera_render_draws_a_back_culled_quad_that_faces_the_camera_through_a_stage_that_negates_clip_y()
 {
    assert_eq!(camera_draw(false), all(DRAWN));
}

#[test]
fn a_camera_render_culls_a_back_culled_quad_whose_indices_are_reversed() {
    assert_eq!(camera_draw(true), all(CLEARED));
}
