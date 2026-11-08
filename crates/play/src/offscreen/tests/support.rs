//! Builds the worlds, snapshots and resources the tests map.

use std::collections::BTreeSet;

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeKernel, ComputeShaderId, MaterialEdit, MaterialId, PropertyValue,
    RenderTextureId, ShaderId, ShaderSource, TextureRef, TextureSlot, WorldComputeShader,
    WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc,
    ObjectHandle, World, WorldSettings,
};

use super::super::defaults::{DefaultTexture, ShaderFill};
use super::super::frame::{Available, ShaderInfo};
use worldrunner_world_simulation::graphics::GraphicsSnapshot;

pub fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

pub fn render_texture(world: &mut World, size: [u32; 2]) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: size[0],
            height: size[1],
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

pub fn shader(world: &mut World, name: &str) -> ShaderId {
    world
        .create_shader(WorldShader {
            name: name.into(),
            source: Some(ShaderSource {
                file: format!("{name}.assets"),
                path_id: 7,
            }),
        })
        .unwrap()
}

pub fn material(world: &mut World, shader: ShaderId) -> MaterialId {
    world.create_material(shader).unwrap()
}

pub fn set(world: &mut World, material: MaterialId, value: PropertyValue) {
    world
        .edit_material(material, MaterialEdit::Property(value))
        .unwrap();
}

pub fn texture_slot(texture: TextureRef) -> TextureSlot {
    TextureSlot {
        texture,
        ..TextureSlot::default()
    }
}

/// An object at `position`, turned by `rotation`, on layer `layer`.
pub fn object(world: &mut World, layer: u8, position: Vec3, rotation: Quat) -> ObjectHandle {
    let desc = ObjectDesc {
        layer,
        local_transform: LocalTransform {
            translation: position,
            rotation,
            scale: Vec3::ONE,
        },
        ..ObjectDesc::default()
    };
    world.create_object(desc).unwrap()
}

pub fn camera(
    world: &mut World,
    settings: Camera,
    position: Vec3,
    rotation: Quat,
) -> ComponentHandle {
    let object = object(world, 0, position, rotation);
    world
        .add_component(object, ComponentDesc::Camera(settings))
        .unwrap()
}

/// A renderer of the mesh stored under `mesh` with one slot for each of `materials`.
pub fn renderer(
    world: &mut World,
    layer: u8,
    mesh: u64,
    materials: Vec<Option<MaterialId>>,
) -> ObjectHandle {
    let renderer = MeshRenderer {
        material_count: u32::try_from(materials.len()).unwrap(),
        slot_materials: materials,
        ..MeshRenderer::default()
    };
    renderer_with(world, (layer, true), mesh, renderer)
}

/// A renderer with the settings given, on an object that is active or not.
pub fn renderer_with(
    world: &mut World,
    (layer, active): (u8, bool),
    mesh: u64,
    renderer: MeshRenderer,
) -> ObjectHandle {
    let desc = ObjectDesc {
        layer,
        active,
        ..ObjectDesc::default()
    };
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh: mesh,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
    object
}

pub fn snapshot(world: &mut World) -> GraphicsSnapshot {
    GraphicsSnapshot::take(world).unwrap()
}

/// Every shader of the snapshot with `passes` passes in the queue 2000, every mesh of its
/// renderers and the stored textures in `textures`.
pub fn available(snapshot: &GraphicsSnapshot, passes: usize, textures: &[u64]) -> Available {
    Available {
        shaders: snapshot
            .shaders
            .keys()
            .map(|id| {
                (
                    *id,
                    ShaderInfo {
                        passes,
                        queue: 2000,
                        texture_defaults: Vec::new(),
                        fill: ShaderFill::default(),
                    },
                )
            })
            .collect(),
        meshes: snapshot.renderers.iter().map(|r| r.mesh).collect(),
        textures: textures.iter().copied().collect::<BTreeSet<_>>(),
        default_textures: DefaultTexture::ALL.into_iter().collect(),
        test_variants: false,
    }
}

/// `matrix * vector` for a column-major matrix, computed by the definition.
pub fn transform(matrix: &[f32; 16], vector: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|row| {
        (0..4)
            .map(|column| matrix[column * 4 + row] * vector[column])
            .sum()
    })
}

/// `left * right` for column-major matrices, computed by the definition.
pub fn product(left: &[f32; 16], right: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|index| {
        let (column, row) = (index / 4, index % 4);
        (0..4)
            .map(|k| left[k * 4 + row] * right[column * 4 + k])
            .sum()
    })
}

pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// The values of a layer's matrix named `name`.
pub fn matrix(layer: &worldrunner_render_program::layers::PropertyLayer, name: &str) -> [f32; 16] {
    match layer.values.get(name) {
        Some(worldrunner_render_program::properties::PropertyValue::Matrix(value)) => *value,
        other => panic!("{name} is {other:?}"),
    }
}

pub fn assert_close(found: &[f32], expected: &[f32]) {
    assert_eq!(found.len(), expected.len());
    for (index, (found, expected)) in found.iter().zip(expected).enumerate() {
        assert!(
            (found - expected).abs() <= 1.0e-5,
            "entry {index}: {found} against {expected} in {found:?} and {expected:?}"
        );
    }
}

/// A compute shader whose compiled kernels are found by `file`, with one kernel `Copy` of
/// thread groups of 2 by 2 by 1.
pub fn compute_shader(world: &mut World, file: &str) -> ComputeShaderId {
    world
        .create_compute_shader(WorldComputeShader {
            name: file.into(),
            source: ShaderSource {
                file: file.into(),
                path_id: 1,
            },
            kernels: vec![ComputeKernel {
                name: "Copy".into(),
                thread_group_size: Some([2, 2, 1]),
            }],
            generation: 0,
        })
        .unwrap()
}

/// A zeroed compute buffer of `count` four-byte elements.
pub fn compute_buffer(world: &mut World, count: u32) -> ComputeBufferId {
    world.create_compute_buffer(count, 4, 0).unwrap()
}

/// Records a dispatch of kernel 0 of `shader` that reads `texture` as `_Heights` and writes
/// `buffer` as `_Output`.
pub fn dispatch(
    world: &mut World,
    shader: ComputeShaderId,
    (texture, buffer): (RenderTextureId, ComputeBufferId),
    groups: [u32; 3],
) {
    world
        .set_compute_property(
            shader,
            Some(0),
            PropertyValue::Texture("_Heights".into(), texture_slot(TextureRef::Render(texture))),
        )
        .unwrap();
    world
        .bind_compute_buffer(shader, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(shader, 0, groups).unwrap();
}
