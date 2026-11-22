//! Where a skinned renderer is drawn and when it is left out. The skin says where: a mesh whose
//! bones are at rest sits at `world(root bone) * bind pose of the root bone`, whatever components
//! the objects above it hold. A skinned renderer with no root bone has no placement and is counted
//! as left out; a mesh renderer is placed at its own object.

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::presentation::{Camera, mesh_objects::extract_mesh_objects};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MeshFilter, MeshRenderer, ObjectHandle, ParentSpace,
    RendererKind, SkinBinding, SkinRoot, World,
};

mod graphics_world;
use graphics_world::{camera, material, object, render_texture, renderer, shader, snapshot, world};

const MESH: u64 = (1 << 41) + 9;

fn same_matrix(a: [f32; 16], b: [f32; 16]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

fn quarter_turn() -> Quat {
    Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)
}

/// An object under `parent` at `position`, turned by `rotation`.
fn child(
    world: &mut World,
    parent: Option<ObjectHandle>,
    (position, rotation): (Vec3, Quat),
) -> ObjectHandle {
    let made = object(world, 0, position, rotation);
    world
        .set_parent(made, parent, ParentSpace::KeepLocal)
        .unwrap();
    made
}

/// An object under `parent` at `position` that holds an animator.
fn animated(world: &mut World, parent: Option<ObjectHandle>, position: Vec3) -> ObjectHandle {
    let made = child(world, parent, (position, Quat::IDENTITY));
    world
        .add_component(made, ComponentDesc::Animator { enabled: true })
        .unwrap();
    made
}

/// A skinned renderer with one material slot on `object`, drawing `mesh` with `root`.
fn add_skin(world: &mut World, object: ObjectHandle, mesh: u64, root: Option<SkinRoot>) {
    let mut renderer = MeshRenderer::of_kind(RendererKind::Skinned);
    renderer.material_count = 1;
    renderer.skin = SkinBinding {
        drawn_mesh: mesh,
        root,
    };
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
}

/// The bind pose of `bone` for a mesh at `body`: the bone's world-to-bone matrix times the mesh's
/// matrix, which is what makes `world(bone) * pose` the mesh's matrix while the bone is at rest.
fn bind_pose(world: &World, bone: ObjectHandle, body: ObjectHandle) -> [f32; 16] {
    (world.world_matrix(bone).unwrap().inverse() * world.world_matrix(body).unwrap())
        .to_cols_array()
}

/// A rig: an animator object at (5, 0, 0) holding a body turned a quarter turn and offset, and a
/// hip bone offset and turned the other way, with the body's skin rooted at the hip. The body
/// has not been given a renderer yet.
struct Rig {
    animator: ObjectHandle,
    body: ObjectHandle,
    hip: ObjectHandle,
    pose: [f32; 16],
}

fn rig(world: &mut World) -> Rig {
    let rig = animated(world, None, Vec3::new(5.0, 0.0, 0.0));
    let body = child(world, Some(rig), (Vec3::new(0.0, 2.0, 0.0), quarter_turn()));
    let hip = child(
        world,
        Some(rig),
        (Vec3::new(1.0, 3.0, 0.0), Quat::from_rotation_x(0.7)),
    );
    let pose = bind_pose(world, hip, body);
    Rig {
        animator: rig,
        body,
        hip,
        pose,
    }
}

impl Rig {
    fn root(&self) -> SkinRoot {
        SkinRoot {
            bone: self.hip,
            bind_pose: self.pose,
        }
    }
}

/// A frame whose one camera render draws every layer.
fn drawn(world: &mut World) -> worldrunner_world_simulation::graphics::GraphicsSnapshot {
    let target = render_texture(world, [8, 8]);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    snapshot(world)
}

#[test]
fn a_skinned_renderer_with_its_bones_at_rest_is_placed_at_its_own_matrix() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, Some(rig.root()));
    let snapshot = drawn(&mut world);
    let [view] = &snapshot.renderers[..] else {
        panic!("one renderer: {:?}", snapshot.renderers)
    };
    assert_eq!(view.mesh, MESH);
    let own = world.world_matrix(rig.body).unwrap().to_cols_array();
    assert!(
        same_matrix(view.local_to_world, own),
        "{:?} against {own:?}",
        view.local_to_world
    );
    let animator = world.world_matrix(rig.animator).unwrap().to_cols_array();
    assert!(
        !same_matrix(view.local_to_world, animator),
        "the animator above the renderer does not place it"
    );
    assert_eq!(snapshot.skinned_not_drawn, 0);
}

#[test]
fn a_skinned_renderer_follows_its_root_bone() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, Some(rig.root()));
    let moved = LocalTransform {
        translation: Vec3::new(4.0, -1.0, 2.0),
        rotation: Quat::from_rotation_y(0.4),
        scale: Vec3::ONE,
    };
    world.set_local_transform(rig.hip, moved).unwrap();
    let expected = world.world_matrix(rig.hip).unwrap() * Mat4::from_cols_array(&rig.pose);
    let snapshot = drawn(&mut world);
    assert!(
        same_matrix(
            snapshot.renderers[0].local_to_world,
            expected.to_cols_array()
        ),
        "the bone's world matrix times its bind pose"
    );
}

#[test]
fn a_skinned_renderer_without_a_root_bone_is_left_out_and_counted_once() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, None);
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    for _ in 0..2 {
        world
            .record_offscreen(OffscreenWork::CameraRender { camera, target })
            .unwrap();
    }
    let snapshot = snapshot(&mut world);
    assert!(snapshot.renderers.is_empty());
    assert_eq!(
        snapshot.skinned_not_drawn, 1,
        "two records name it, the count is of renderers"
    );
}

#[test]
fn a_skinned_renderer_whose_root_bone_is_destroyed_is_left_out_and_counted() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, Some(rig.root()));
    world.destroy_object(rig.hip).unwrap();
    let snapshot = drawn(&mut world);
    assert!(snapshot.renderers.is_empty());
    assert_eq!(snapshot.skinned_not_drawn, 1);
}

#[test]
fn a_skinned_renderer_that_names_no_mesh_is_neither_listed_nor_counted() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, 0, Some(rig.root()));
    let snapshot = drawn(&mut world);
    assert!(snapshot.renderers.is_empty());
    assert_eq!(snapshot.skinned_not_drawn, 0);
}

#[test]
fn a_mesh_renderer_below_an_animator_is_placed_at_its_own_matrix() {
    let mut world = world();
    let animator = animated(&mut world, None, Vec3::new(5.0, 0.0, 0.0));
    let held = renderer(&mut world, 0, MESH, vec![None]);
    world
        .set_parent(held, Some(animator), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_local_transform(
            held,
            LocalTransform {
                translation: Vec3::new(0.0, 7.0, 0.0),
                rotation: quarter_turn(),
                scale: Vec3::ONE,
            },
        )
        .unwrap();
    let snapshot = drawn(&mut world);
    assert!(same_matrix(
        snapshot.renderers[0].local_to_world,
        world.world_matrix(held).unwrap().to_cols_array()
    ));
}

#[test]
fn a_skinned_view_carries_the_materials_of_its_slots() {
    let mut world = world();
    let rig = rig(&mut world);
    let shader = shader(&mut world, "Skin");
    let material = material(&mut world, shader);
    let mut skinned = MeshRenderer::of_kind(RendererKind::Skinned);
    skinned.material_count = 2;
    skinned.slot_materials = vec![None, Some(material)];
    skinned.skin = SkinBinding {
        drawn_mesh: MESH,
        root: Some(rig.root()),
    };
    world
        .add_component(rig.body, ComponentDesc::MeshRenderer(skinned))
        .unwrap();
    let snapshot = drawn(&mut world);
    assert_eq!(snapshot.renderers[0].materials, [None, Some(material)]);
    assert!(snapshot.materials.contains_key(&material));
}

#[test]
fn a_skinned_renderer_draws_its_own_mesh_and_not_its_objects_filter() {
    let mut world = world();
    renderer(&mut world, 0, MESH, vec![]);
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH + 1, Some(rig.root()));
    world
        .add_component(
            rig.body,
            ComponentDesc::MeshFilter(MeshFilter {
                mesh: 3,
                drawn_mesh: MESH + 2,
            }),
        )
        .unwrap();
    let meshes: Vec<u64> = drawn(&mut world)
        .renderers
        .iter()
        .map(|view| view.mesh)
        .collect();
    assert_eq!(meshes, [MESH, MESH + 1]);
}

/// The matrix a listed mesh object of `object` has: the object's own matrix in the frame's view
/// basis, which turns the world's up axis into the view's.
fn in_view_basis(world: &World, object: ObjectHandle) -> [f32; 16] {
    let basis = Mat4::from_cols_array(&[
        1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.,
    ]);
    (basis * world.world_matrix(object).unwrap()).to_cols_array()
}

fn listed_matrix(world: &World, object: ObjectHandle) -> Option<[f32; 16]> {
    extract_mesh_objects(world)
        .unwrap()
        .iter()
        .find(|listed| listed.object == object)
        .map(|listed| listed.local_to_world)
}

#[test]
fn the_presentation_list_places_a_skinned_renderer_by_its_root_bone() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, Some(rig.root()));
    let placed = listed_matrix(&world, rig.body).expect("the skinned renderer is listed");
    assert!(same_matrix(placed, in_view_basis(&world, rig.body)));
}

#[test]
fn the_presentation_list_leaves_out_a_skinned_renderer_with_no_root_bone() {
    let mut world = world();
    let rig = rig(&mut world);
    add_skin(&mut world, rig.body, MESH, None);
    assert!(listed_matrix(&world, rig.body).is_none());
}

#[test]
fn the_presentation_list_places_a_mesh_renderer_below_an_animator_at_itself() {
    let mut world = world();
    let animator = animated(&mut world, None, Vec3::new(5.0, 0.0, 0.0));
    let held = renderer(&mut world, 0, MESH, vec![None]);
    world
        .set_parent(held, Some(animator), ParentSpace::KeepLocal)
        .unwrap();
    let placed = listed_matrix(&world, held).expect("the mesh renderer is listed");
    assert!(same_matrix(placed, in_view_basis(&world, held)));
}
