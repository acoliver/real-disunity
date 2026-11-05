//! The synchronous GPU flush on a real adapter (lavapipe in CI): a camera render into a height
//! map, a dispatch that copies it into a compute buffer, and the bytes the world gets back.
//!
//! The kernel is the shape of the terrain read-back: it reads an R16 float render texture and
//! writes a storage buffer, and a vector in a constant buffer scales the values. Every expected
//! value comes from the tint, the clear color and the scale, not from the output.

use glam::{Quat, Vec3};
use worldrunner_render_program::module::StageProgram;
use worldrunner_render_program::synthetic::{
    binding, compute_stage, constant_buffer, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeShaderId, OffscreenWork, PropertyValue, RenderTextureId,
    ShaderProperties, WorldComputeShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::adapter::OffscreenAdapter;
use super::super::compute::{ComputeAdapter, ComputeLoadError, ComputeSupply};
use super::super::flush::OffscreenHost;
use super::super::frame::Settings;
use super::gpu::{MESH, Quad, SIDE, Synthetic, gpu, shader, target, vector};
use super::support::{
    camera, compute_buffer, compute_shader, dispatch, renderer_with, snapshot, world,
};

/// `GraphicsFormat` 45, one 16-bit float channel.
const HEIGHT: u32 = 45;
/// The red the camera's material draws over the left half, and the red it clears to.
const DRAWN: f32 = 0.5;
const CLEAR: f32 = 0.25;
/// What the kernel multiplies every height by.
const SCALE: f32 = 2.0;

const KERNEL: &str = "
struct Params { scale: vec4<f32> }
@group(0) @binding(0) var heights: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    output[id.y * 4u + id.x] = textureLoad(heights, vec2<i32>(id.xy), 0).x * params.scale.x;
}";

fn copy_kernel() -> StageProgram {
    let parameters = VariantParameters {
        constant_buffers: vec![constant_buffer(
            "Params",
            16,
            vec![vector_member("_Scale", 0)],
        )],
        bindings: vec![
            binding(
                "_Heights",
                slot(0, 0, 0),
                ResourceKind::Texture {
                    sampler: None,
                    dimension: 4,
                },
            ),
            binding("_Output", slot(0, 1, 0), ResourceKind::Buffer),
            binding("Params", slot(0, 2, 0), ResourceKind::ConstantBuffer),
        ],
    };
    compute_stage(KERNEL, "main", &[(0, 0), (0, 1), (0, 2)], parameters)
}

/// The compiled kernels of the shader whose source file is `heights`.
struct Kernels;

impl ComputeSupply for Kernels {
    fn load(
        &mut self,
        shader: &WorldComputeShader,
        _: u32,
    ) -> Result<StageProgram, ComputeLoadError> {
        match shader.source.file.as_str() {
            "heights" => Ok(copy_kernel()),
            _ => Err(ComputeLoadError::NoCompiledCode),
        }
    }
}

fn host() -> OffscreenHost {
    let gpu = gpu();
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
    };
    OffscreenHost::new(
        gpu.runtime,
        gpu.arena,
        OffscreenAdapter::with_supply(Synthetic, settings),
        ComputeAdapter::new(Kernels),
    )
}

/// A camera that clears to `CLEAR` in red and draws `DRAWN` over the left half of its target.
fn camera_render(world: &mut World, into: RenderTextureId) {
    let camera = camera(
        world,
        Camera {
            background: [f64::from(CLEAR), 0.0, 1.0, 1.0],
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let shader = shader(world, "camera");
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", [DRAWN, 0.0, 0.0, 1.0]);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector(
            "_Place".into(),
            [0.5, 1.0, -0.5, 0.0],
        ))
        .unwrap();
    let placed = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, placed);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: into,
        })
        .unwrap();
}

/// A compute shader the kernels are found for, scaling by `SCALE`.
fn scaling(world: &mut World) -> ComputeShaderId {
    let shader = compute_shader(world, "heights");
    world
        .set_compute_property(
            shader,
            None,
            PropertyValue::Vector("_Scale".into(), [SCALE, 0.0, 0.0, 0.0]),
        )
        .unwrap();
    shader
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

/// The values the kernel writes for a height map the camera drew: the left two columns of each
/// row are drawn, the rest cleared.
fn expected() -> Vec<f32> {
    (0..SIDE * SIDE)
        .map(|index| {
            if index % SIDE < SIDE / 2 {
                DRAWN * SCALE
            } else {
                CLEAR * SCALE
            }
        })
        .collect()
}

#[test]
fn a_camera_render_into_a_height_map_then_a_dispatch_that_copies_it_writes_the_buffer_back() {
    let mut host = host();
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert_eq!(outcome.writes.len(), 1);
    outcome.apply(&mut world).unwrap();

    // Left columns: 0.5 * 2 = 1, right columns: 0.25 * 2 = 0.5, all exact in 16-bit floats.
    assert_eq!(floats(&world, buffer), expected());
    assert_eq!(
        floats(&world, buffer).iter().filter(|v| **v != 0.0).count(),
        16,
        "every height is non-zero"
    );
}

#[test]
fn work_runs_in_the_order_it_was_called() {
    let mut host = host();
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let shader = scaling(&mut world);
    let (before, after) = (
        compute_buffer(&mut world, SIDE * SIDE),
        compute_buffer(&mut world, SIDE * SIDE),
    );
    // The first dispatch reads the height map before the camera draws it, the second after.
    dispatch(&mut world, shader, (heights, before), [2, 2, 1]);
    camera_render(&mut world, heights);
    dispatch(&mut world, shader, (heights, after), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    let written: Vec<_> = outcome.writes.iter().map(|write| write.id).collect();
    assert_eq!(written, [after], "the first buffer still holds zeros");
    outcome.apply(&mut world).unwrap();
    assert_eq!(floats(&world, before), vec![0.0; 16]);
    assert_eq!(floats(&world, after), expected());
}

#[test]
fn a_flush_with_no_work_writes_nothing() {
    let mut host = host();
    let mut world = world();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(outcome.writes.is_empty() && outcome.report.failures.is_empty());
}

#[test]
fn a_dispatch_that_cannot_run_is_left_out_by_the_name_of_the_cause() {
    let mut host = host();
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    // A shader whose kernels the supply does not have.
    let unknown = compute_shader(&mut world, "missing");
    dispatch(&mut world, unknown, (heights, buffer), [2, 2, 1]);
    // A kernel that names a texture the dispatch does not set.
    let bare = scaling(&mut world);
    world
        .bind_compute_buffer(bare, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(bare, 0, [2, 2, 1]).unwrap();
    // A kernel with no buffer bound to the name it declares.
    let loose = scaling(&mut world);
    world
        .set_compute_property(
            loose,
            Some(0),
            PropertyValue::Texture(
                "_Heights".into(),
                super::support::texture_slot(
                    worldrunner_world_simulation::graphics::TextureRef::Render(heights),
                ),
            ),
        )
        .unwrap();
    world.dispatch_compute(loose, 0, [2, 2, 1]).unwrap();
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    let failures: Vec<_> = outcome
        .report
        .failures
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect();
    assert_eq!(
        failures,
        [
            ("dispatch:MissingTexture", 1),
            ("dispatch:NoCompiledCode", 1),
            ("dispatch:UnboundBuffer", 1),
        ]
    );
    assert!(outcome.writes.is_empty());
}
