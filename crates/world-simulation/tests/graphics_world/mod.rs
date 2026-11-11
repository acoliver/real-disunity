//! Builds the worlds the graphics snapshot tests take from.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, MaterialId, RenderTextureId, ShaderId, ShaderSource, WorldRenderTexture,
    WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc,
    ObjectHandle, World, WorldSettings,
};

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
            dimension: 2,
            volume_depth: 1,
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
