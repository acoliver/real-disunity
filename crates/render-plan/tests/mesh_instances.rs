#![cfg(feature = "simulation")]
use glam::{Quat, Vec3};
use worldrunner_render_plan::simulation::mesh_instances::{
    MeshExtractError, MeshResolver, ResolvedMesh, extract_mesh_instances, extract_mesh_object,
};
use worldrunner_render_plan::{MaterialHandle, MeshHandle};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldSettings,
};

/// Mesh id n lives at table index n, and material id m at index m - 100.
fn table() -> impl MeshResolver {
    |instance: &MeshInstance| {
        Some(ResolvedMesh {
            mesh: MeshHandle::from_raw(u32::try_from(instance.mesh.0).ok()?),
            material: MaterialHandle::from_raw(u32::try_from(instance.materials[0].0 - 100).ok()?),
        })
    }
}

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn drawn(world: &mut World, mesh: u64, local: LocalTransform) -> ObjectHandle {
    let object = world
        .create_object(ObjectDesc {
            local_transform: local,
            ..ObjectDesc::default()
        })
        .unwrap();
    let instance = MeshInstance::new(MeshId(mesh), MaterialId(100 + mesh));
    world
        .add_component(object, ComponentDesc::MeshInstance(instance))
        .unwrap();
    object
}

fn moved(translation: Vec3) -> LocalTransform {
    LocalTransform {
        translation,
        ..LocalTransform::default()
    }
}

#[test]
fn two_level_hierarchy_extracts_composed_world_transforms() {
    let mut w = world();
    let parent = drawn(
        &mut w,
        1,
        LocalTransform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(2.0),
        },
    );
    let child = drawn(
        &mut w,
        2,
        LocalTransform {
            translation: Vec3::X,
            scale: Vec3::splat(0.5),
            ..LocalTransform::default()
        },
    );
    w.set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();

    let extracted = extract_mesh_instances(&w, &table()).unwrap();
    assert_eq!(extracted.len(), 2);

    let p = &extracted[0];
    assert_eq!(p.object, parent);
    let t = p.scene.transform();
    assert_eq!(t.position_millimeters, [1000, 2000, 3000]);
    assert_eq!(t.scale_millionths, 2_000_000);
    assert_eq!((p.scene.mesh().raw(), p.scene.material().raw()), (1, 1));

    // The parent's 2x scale and quarter turn carry the child's local +X to world +Y.
    let c = &extracted[1];
    assert_eq!(c.object, child);
    let t = c.scene.transform();
    assert_eq!(t.position_millimeters, [1000, 4000, 3000]);
    assert_eq!(t.scale_millionths, 1_000_000);
    let half_root = 707_107;
    assert!((t.rotation.z - half_root).abs() <= 1 && (t.rotation.w - half_root).abs() <= 1);
    assert_eq!((t.rotation.x, t.rotation.y), (0, 0));
    assert_eq!((c.scene.mesh().raw(), c.scene.material().raw()), (2, 2));
}

#[test]
fn tint_reaches_the_scene_object_in_micro_units() {
    let mut w = world();
    let object = drawn(&mut w, 1, LocalTransform::default());
    let mut instance = w.mesh_instance(object).unwrap().unwrap().clone();
    instance.tint = [0.5, 1.0, 2.0];
    let component = w
        .snapshot()
        .components
        .into_iter()
        .find(|c| matches!(c.desc, ComponentDesc::MeshInstance(_)))
        .unwrap()
        .handle;
    w.set_component(component, ComponentDesc::MeshInstance(instance))
        .unwrap();
    let scene = extract_mesh_object(&w, object, &table()).unwrap().unwrap();
    assert_eq!(scene.tint().channels(), [500_000, 1_000_000, 2_000_000]);
}

#[test]
fn inactive_and_undrawn_objects_are_not_extracted() {
    let mut w = world();
    let parent = drawn(&mut w, 1, LocalTransform::default());
    let child = drawn(&mut w, 2, LocalTransform::default());
    let bare = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    w.set_active(parent, false).unwrap();
    assert!(extract_mesh_instances(&w, &table()).unwrap().is_empty());
    assert!(extract_mesh_object(&w, bare, &table()).unwrap().is_none());
}

#[test]
fn unresolved_ids_and_unrepresentable_transforms_are_errors() {
    let mut w = world();
    let object = drawn(&mut w, 1, LocalTransform::default());
    let nothing = |_: &MeshInstance| None;
    assert_eq!(
        extract_mesh_instances(&w, &nothing).unwrap_err(),
        MeshExtractError::Unresolved(object)
    );

    let skewed = drawn(
        &mut w,
        2,
        LocalTransform {
            scale: Vec3::new(1.0, 2.0, 1.0),
            ..LocalTransform::default()
        },
    );
    assert!(matches!(
        extract_mesh_object(&w, skewed, &table()),
        Err(MeshExtractError::Transform { object, .. }) if object == skewed
    ));
    let flat = drawn(
        &mut w,
        3,
        LocalTransform {
            scale: Vec3::ZERO,
            ..moved(Vec3::ONE)
        },
    );
    assert!(matches!(
        extract_mesh_object(&w, flat, &table()),
        Err(MeshExtractError::Transform { .. })
    ));
}
