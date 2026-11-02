//! Which mesh renderers a frame lists and what each carries: the renderer must be enabled, on an
//! active object, with a mesh the host holds, on a layer the presented camera admits.
use glam::Vec3;
use worldrunner_world_simulation::presentation::{Camera, mesh_objects::extract_mesh_objects};
use worldrunner_world_simulation::{
    ComponentDesc, DrawnMaterial, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc,
    ObjectHandle, RendererKind, World, WorldSettings,
};

const MESH: u64 = (1 << 41) + 5;

fn new_world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn drawn(red: f32, texture: u64) -> DrawnMaterial {
    DrawnMaterial {
        color: [red, 0.5, 0.25, 1.0],
        texture,
    }
}

fn renderer(slots: Vec<DrawnMaterial>) -> MeshRenderer {
    MeshRenderer {
        material_count: u32::try_from(slots.len()).unwrap(),
        drawn: slots,
        ..MeshRenderer::default()
    }
}

/// An object with a filter naming `drawn_mesh` and `renderer`.
fn add_mesh_object(
    world: &mut World,
    drawn_mesh: u64,
    renderer: MeshRenderer,
    desc: ObjectDesc,
) -> ObjectHandle {
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
    object
}

fn simple(world: &mut World) -> ObjectHandle {
    add_mesh_object(
        world,
        MESH,
        renderer(vec![drawn(1.0, 0)]),
        ObjectDesc::default(),
    )
}

fn listed(world: &World) -> Vec<ObjectHandle> {
    extract_mesh_objects(world)
        .unwrap()
        .into_iter()
        .map(|o| o.object)
        .collect()
}

fn add_camera(world: &mut World, culling_mask: u32) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let camera = Camera {
        culling_mask,
        ..Camera::default()
    };
    world
        .add_component(object, ComponentDesc::Camera(camera))
        .unwrap();
}

#[test]
fn an_enabled_renderer_with_a_drawn_mesh_is_listed_with_its_mesh_and_slots() {
    let mut world = new_world();
    let slots = vec![drawn(0.1, 0), drawn(0.2, (1 << 40) + 3)];
    let object = add_mesh_object(
        &mut world,
        MESH,
        renderer(slots.clone()),
        ObjectDesc::default(),
    );
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].object, listed[0].mesh), (object, MESH));
    assert_eq!(listed[0].slots, slots);
}

#[test]
fn a_renderer_without_a_drawn_mesh_or_a_filter_is_left_out() {
    let mut world = new_world();
    add_mesh_object(
        &mut world,
        0,
        renderer(vec![drawn(1.0, 0)]),
        ObjectDesc::default(),
    );
    let bare = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(bare, ComponentDesc::MeshRenderer(MeshRenderer::default()))
        .unwrap();
    assert!(listed(&world).is_empty());
}

#[test]
fn a_disabled_renderer_or_one_forced_off_is_left_out_and_the_others_stay() {
    let mut world = new_world();
    let mut disabled = renderer(vec![drawn(1.0, 0)]);
    disabled.switches.enabled = false;
    let mut forced_off = renderer(vec![drawn(1.0, 0)]);
    forced_off.switches.force_rendering_off = true;
    add_mesh_object(&mut world, MESH, disabled, ObjectDesc::default());
    add_mesh_object(&mut world, MESH, forced_off, ObjectDesc::default());
    let shown = simple(&mut world);
    assert_eq!(listed(&world), vec![shown]);
}

#[test]
fn skinned_and_particle_renderers_are_left_out() {
    let mut world = new_world();
    for kind in [RendererKind::Skinned, RendererKind::Particle] {
        let renderer = MeshRenderer::of_kind(kind);
        add_mesh_object(&mut world, MESH, renderer, ObjectDesc::default());
    }
    assert!(listed(&world).is_empty());
}

#[test]
fn an_inactive_object_or_a_child_of_one_is_left_out_until_it_is_activated() {
    let mut world = new_world();
    let object = simple(&mut world);
    world.set_active(object, false).unwrap();
    assert!(listed(&world).is_empty());
    world.set_active(object, true).unwrap();
    assert_eq!(listed(&world), vec![object]);
}

#[test]
fn the_cameras_culling_mask_decides_by_layer() {
    let mut world = new_world();
    let on_layer_five = ObjectDesc {
        layer: 5,
        ..ObjectDesc::default()
    };
    let five = add_mesh_object(
        &mut world,
        MESH,
        renderer(vec![drawn(1.0, 0)]),
        on_layer_five,
    );
    let zero = simple(&mut world);
    assert_eq!(listed(&world), vec![five, zero], "no camera culls nothing");
    add_camera(&mut world, 1 << 5);
    assert_eq!(listed(&world), vec![five]);
}

#[test]
fn slots_are_one_per_material_and_a_missing_one_draws_white() {
    let mut world = new_world();
    let mut three = renderer(vec![drawn(0.1, 0)]);
    three.material_count = 3;
    add_mesh_object(&mut world, MESH, three, ObjectDesc::default());
    let none = renderer(vec![]);
    add_mesh_object(&mut world, MESH, none, ObjectDesc::default());
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(
        listed[0].slots,
        vec![
            drawn(0.1, 0),
            DrawnMaterial::default(),
            DrawnMaterial::default()
        ]
    );
    assert!(listed[1].slots.is_empty());
}

#[test]
fn the_matrix_places_the_object_in_the_view_basis() {
    let mut world = new_world();
    let desc = ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            ..LocalTransform::default()
        },
        ..ObjectDesc::default()
    };
    add_mesh_object(&mut world, MESH, renderer(vec![drawn(1.0, 0)]), desc);
    let matrix = extract_mesh_objects(&world).unwrap()[0].local_to_world;
    // The engine's y becomes the view's up, and its z the view's depth: (x, y, z) -> (x, -z, y).
    assert_eq!(&matrix[12..15], &[1.0, -3.0, 2.0]);
    assert_eq!(&matrix[0..3], &[1.0, 0.0, 0.0]);
    assert_eq!(&matrix[4..7], &[0.0, 0.0, 1.0]);
    assert_eq!(&matrix[8..11], &[0.0, -1.0, 0.0]);
}
