//! What a listed mesh object carries for a host that draws it with its own shaders: the layer,
//! the world material of each slot, the renderer's property block and queue, and the camera
//! target a camera draws into.
use worldrunner_world_simulation::graphics::{
    MaterialId, PropertyValue, RenderTextureId, ShaderProperties, WorldRenderTexture, WorldShader,
};
use worldrunner_world_simulation::presentation::{Camera, mesh_objects::extract_mesh_objects};
use worldrunner_world_simulation::{
    ComponentDesc, MeshFilter, MeshRenderer, ObjectDesc, ObjectHandle, World, WorldSettings,
};

const MESH: u64 = (1 << 41) + 5;

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Terrain".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn add(world: &mut World, layer: u8, renderer: MeshRenderer) -> ObjectHandle {
    let desc = ObjectDesc {
        layer,
        ..ObjectDesc::default()
    };
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh: MESH,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
    object
}

#[test]
fn a_mesh_object_carries_its_objects_layer() {
    let mut world = world();
    add(&mut world, 9, MeshRenderer::default());
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed[0].layer, 9);
}

#[test]
fn a_mesh_object_lists_the_world_material_of_each_slot() {
    let mut world = world();
    let first = material(&mut world);
    let renderer = MeshRenderer {
        material_count: 3,
        slot_materials: vec![Some(first), None],
        ..MeshRenderer::default()
    };
    add(&mut world, 0, renderer);
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed[0].materials, [Some(first), None, None]);
    assert_eq!(listed[0].slots.len(), 3);
}

#[test]
fn a_destroyed_material_reads_as_no_material() {
    let mut world = world();
    let first = material(&mut world);
    let second = material(&mut world);
    let renderer = MeshRenderer {
        material_count: 2,
        slot_materials: vec![Some(first), Some(second)],
        ..MeshRenderer::default()
    };
    add(&mut world, 0, renderer);
    world.destroy_material(first).unwrap();
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed[0].materials, [None, Some(second)]);
}

#[test]
fn a_mesh_object_carries_the_property_block_and_queue_override() {
    let mut world = world();
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector("_Tint".into(), [1.0, 0.5, 0.25, 1.0]))
        .unwrap();
    let renderer = MeshRenderer {
        property_block: Some(block.clone()),
        render_queue: Some(2450),
        ..MeshRenderer::default()
    };
    add(&mut world, 0, renderer);
    add(&mut world, 0, MeshRenderer::default());
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed[0].property_block, Some(block));
    assert_eq!(listed[0].render_queue, Some(2450));
    assert_eq!(listed[1].property_block, None);
    assert_eq!(listed[1].render_queue, None);
}

#[test]
fn a_renderer_cannot_name_more_slot_materials_than_it_has_slots() {
    let mut world = world();
    let one = material(&mut world);
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(one), None],
        ..MeshRenderer::default()
    };
    assert!(
        world
            .add_component(object, ComponentDesc::MeshRenderer(renderer))
            .is_err()
    );
}

fn target(world: &mut World) -> RenderTextureId {
    let texture = WorldRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        depth_bits: 0,
        mip_count: 1,
        srgb: false,
        generation: 0,
    };
    world.create_render_texture(texture).unwrap()
}

#[test]
fn a_camera_keeps_the_render_texture_it_draws_into() {
    let mut world = world();
    let texture = target(&mut world);
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let camera = Camera {
        target_texture: Some(texture),
        ..Camera::default()
    };
    let handle = world
        .add_component(object, ComponentDesc::Camera(camera))
        .unwrap();
    match world.component(handle).unwrap().desc {
        ComponentDesc::Camera(held) => assert_eq!(held.target_texture, Some(texture)),
        _ => panic!("the component is a camera"),
    }
    assert!(!camera.draws_to_screen());
    assert!(Camera::default().draws_to_screen());
}
