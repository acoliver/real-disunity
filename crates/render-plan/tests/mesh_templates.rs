#![cfg(feature = "simulation")]
use glam::{Quat, Vec3};
use worldrunner_render_plan::simulation::mesh_instances::{
    MeshExtractError, MeshExtractOptions, MeshResolver, MeshTemplate, ResolvedMesh,
    extract_mesh_instances, extract_mesh_instances_with, extract_mesh_object, extract_mesh_objects,
};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneFadeBand, SceneObject, SceneRotation, SceneTint,
    SceneTransform,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle,
    World, WorldSettings,
};

const IDENTITY: SceneTransform = SceneTransform {
    position_millimeters: [0; 3],
    rotation: SceneRotation::IDENTITY,
    scale_millionths: 1_000_000,
};

/// A host that draws every instance as `layers` objects with different policies, the way a host
/// draws LOD levels and an impostor card for one world object.
struct Layered {
    layers: usize,
}

fn layer(index: u32) -> SceneObject {
    SceneObject::new(
        IDENTITY,
        MeshHandle::from_raw(index),
        MaterialHandle::from_raw(7),
    )
    .with_lod_distance_scale_millionths(1_500_000 + index)
    .unwrap()
    .with_fade(SceneFadeBand::new(1_000 * (index + 1), 2_000 * (index + 1)).unwrap())
    .with_tint(SceneTint::new([250_000, 500_000 + index, 750_000]).unwrap())
    .with_casts_shadow(index == 0)
}

impl MeshResolver for Layered {
    fn resolve(&self, _: &MeshInstance) -> Option<ResolvedMesh> {
        None
    }

    fn resolve_template(&self, _: &MeshInstance) -> Option<MeshTemplate> {
        let layers = (0..u32::try_from(self.layers).unwrap())
            .map(layer)
            .collect();
        Some(MeshTemplate::Objects(layers))
    }
}

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn drawn(world: &mut World, local: LocalTransform) -> ObjectHandle {
    let object = world
        .create_object(ObjectDesc {
            local_transform: local,
            ..ObjectDesc::default()
        })
        .unwrap();
    let instance = MeshInstance::new(MeshId(1), MaterialId(1));
    world
        .add_component(object, ComponentDesc::MeshInstance(instance))
        .unwrap();
    object
}

fn at(x: f32, y: f32, z: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3::new(x, y, z),
        ..LocalTransform::default()
    }
}

fn units(millimeters_per_unit: f64) -> MeshExtractOptions {
    MeshExtractOptions::with_millimeters_per_unit(millimeters_per_unit).unwrap()
}

fn only(world: &World, object: ObjectHandle, options: MeshExtractOptions) -> SceneObject {
    let mut scenes = extract_mesh_objects(world, object, &Layered { layers: 1 }, options).unwrap();
    assert_eq!(scenes.len(), 1);
    scenes.remove(0)
}

#[test]
fn template_fields_survive_and_only_the_transform_changes() {
    let mut w = world();
    let object = drawn(
        &mut w,
        LocalTransform {
            translation: Vec3::new(1.5, -2.25, 3.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(2.0),
        },
    );
    let scene = extract_mesh_object(&w, object, &Layered { layers: 1 })
        .unwrap()
        .unwrap();
    let template = layer(0);
    assert_eq!(scene.lod_distance_scale_millionths(), Some(1_500_000));
    assert_eq!(scene.fade(), template.fade());
    assert_eq!(scene.tint(), template.tint());
    assert!(scene.casts_shadow());
    assert_eq!(
        (scene.mesh(), scene.material()),
        (template.mesh(), template.material())
    );
    let t = scene.transform();
    assert_eq!(t.position_millimeters, [1500, -2250, 3000]);
    assert_eq!(t.scale_millionths, 2_000_000);
    assert_ne!(t.rotation, SceneRotation::IDENTITY);
    // Everything but the transform equals the template.
    assert_eq!(scene.with_transform(IDENTITY), template);
}

#[test]
fn handle_resolvers_still_take_their_tint_from_the_instance() {
    let mut w = world();
    let object = drawn(&mut w, at(1.0, 0.0, 0.0));
    let handles = |_: &MeshInstance| {
        Some(ResolvedMesh {
            mesh: MeshHandle::from_raw(3),
            material: MaterialHandle::from_raw(4),
        })
    };
    let scene = extract_mesh_object(&w, object, &handles).unwrap().unwrap();
    assert_eq!(scene.tint(), SceneTint::WHITE);
    assert_eq!(scene.lod_distance_scale_millionths(), None);
    assert_eq!(scene.transform().position_millimeters, [1000, 0, 0]);
}

#[test]
fn a_millimeter_world_gives_the_same_transform_as_a_meter_world() {
    let rotation = Quat::from_rotation_z(0.7);
    let meters = LocalTransform {
        translation: Vec3::new(1.5, -2.25, 3.0),
        rotation,
        scale: Vec3::splat(2.0),
    };
    let millimeters = LocalTransform {
        translation: Vec3::new(1500.0, -2250.0, 3000.0),
        ..meters
    };
    let (mut wm, mut wmm) = (world(), world());
    let (a, b) = (drawn(&mut wm, meters), drawn(&mut wmm, millimeters));
    let in_meters = only(&wm, a, MeshExtractOptions::default());
    let in_millimeters = only(&wmm, b, units(1.0));
    assert_eq!(in_meters.transform(), in_millimeters.transform());
    assert_eq!(in_millimeters.transform().scale_millionths, 2_000_000);
}

#[test]
fn whole_units_round_trip_exactly_up_to_two_to_the_24th() {
    let limit = 16_777_216.0_f32;
    let mut w = world();
    let object = drawn(&mut w, at(limit, -(limit - 1.0), 1.0));
    let t = *only(&w, object, units(1.0)).transform();
    assert_eq!(t.position_millimeters, [16_777_216, -16_777_215, 1]);

    let t = *only(&w, object, units(1000.0)).transform();
    assert_eq!(t.position_millimeters[0], 16_777_216_000);
    let t = *only(&w, object, units(10.0)).transform();
    assert_eq!(t.position_millimeters[1], -167_772_150);
}

#[test]
fn units_apply_to_translation_and_never_to_scale() {
    let mut w = world();
    let object = drawn(
        &mut w,
        LocalTransform {
            scale: Vec3::splat(3.0),
            ..at(2.0, 0.0, 0.0)
        },
    );
    for (per_unit, expected) in [(0.5, 1), (1.0, 2), (10.0, 20), (1000.0, 2000)] {
        let t = *only(&w, object, units(per_unit)).transform();
        assert_eq!(t.scale_millionths, 3_000_000);
        assert_eq!(t.position_millimeters[0], expected);
    }
    let default = *only(&w, object, MeshExtractOptions::default()).transform();
    assert_eq!(default.position_millimeters[0], 2_000);
}

#[test]
fn positions_out_of_range_and_bad_units_are_errors() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            MeshExtractOptions::with_millimeters_per_unit(bad),
            Err(MeshExtractError::InvalidUnit(_))
        ));
    }
    let mut w = world();
    let far = drawn(&mut w, at(1.0e16, 0.0, 0.0));
    let result = extract_mesh_objects(&w, far, &Layered { layers: 1 }, units(1.0));
    assert!(matches!(
        result,
        Err(MeshExtractError::Transform { object, .. }) if object == far
    ));
    let result = extract_mesh_object(&w, far, &Layered { layers: 1 });
    assert!(matches!(result, Err(MeshExtractError::Transform { .. })));
}

#[test]
fn one_instance_extracts_every_template_in_order() {
    let mut w = world();
    let first = drawn(&mut w, at(1.0, 0.0, 0.0));
    let second = drawn(&mut w, at(2.0, 0.0, 0.0));
    let resolver = Layered { layers: 3 };

    let extracted = extract_mesh_instances(&w, &resolver).unwrap();
    let owners: Vec<_> = extracted.iter().map(|e| e.object).collect();
    assert_eq!(owners, [first, first, first, second, second, second]);
    let meshes: Vec<_> = extracted.iter().map(|e| e.scene.mesh().raw()).collect();
    assert_eq!(meshes, [0, 1, 2, 0, 1, 2]);
    assert!(
        extracted[3..]
            .iter()
            .all(|e| e.scene.transform().position_millimeters == [2000, 0, 0])
    );
    let with_default = extract_mesh_instances_with(&w, &resolver, MeshExtractOptions::default());
    assert_eq!(with_default.unwrap().len(), extracted.len());
    assert_eq!(
        extract_mesh_object(&w, first, &resolver).unwrap_err(),
        MeshExtractError::SeveralObjects(first)
    );
}

#[test]
fn an_empty_template_list_draws_nothing() {
    let mut w = world();
    let object = drawn(&mut w, LocalTransform::default());
    assert!(
        extract_mesh_instances(&w, &Layered { layers: 0 })
            .unwrap()
            .is_empty()
    );
    assert!(
        extract_mesh_object(&w, object, &Layered { layers: 0 })
            .unwrap()
            .is_none()
    );
}

/// A host with its own placement policy: the scene transform comes from the instance's mesh id,
/// not from the world transform, and the first object keeps a distinct policy set.
struct Placer;

fn placed_transform(index: u32, mesh: u64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [i64::try_from(mesh).unwrap() * 7, -3, i64::from(index)],
        rotation: SceneRotation {
            x: 0,
            y: 707_107,
            z: 0,
            w: 707_107,
        },
        scale_millionths: 2_500_000 + u64::from(index),
    }
}

impl MeshResolver for Placer {
    fn resolve(&self, _: &MeshInstance) -> Option<ResolvedMesh> {
        None
    }

    fn resolve_template(&self, instance: &MeshInstance) -> Option<MeshTemplate> {
        let placed = (0..2)
            .map(|index| layer(index).with_transform(placed_transform(index, instance.mesh.0)))
            .collect();
        Some(MeshTemplate::Placed(placed))
    }
}

#[test]
fn placed_templates_come_out_exactly_as_the_host_built_them() {
    let mut w = world();
    let object = drawn(&mut w, at(1.5, -2.25, 3.0));
    let scenes = extract_mesh_objects(&w, object, &Placer, MeshExtractOptions::default()).unwrap();
    let expected: Vec<_> = (0..2)
        .map(|index| layer(index).with_transform(placed_transform(index, 1)))
        .collect();
    assert_eq!(scenes, expected);

    let meter = extract_mesh_instances_with(&w, &Placer, units(1.0)).unwrap();
    assert_eq!(meter.len(), 2);
    assert!(meter.iter().all(|e| e.object == object));
    assert_eq!(meter.iter().map(|e| e.scene).collect::<Vec<_>>(), expected);
}

#[test]
fn placed_templates_ignore_a_world_transform_a_scene_object_cannot_hold() {
    let mut w = world();
    let object = drawn(
        &mut w,
        LocalTransform {
            scale: Vec3::new(1.0, 2.0, 3.0),
            ..LocalTransform::default()
        },
    );
    assert!(matches!(
        extract_mesh_objects(&w, object, &Layered { layers: 1 }, units(1.0)),
        Err(MeshExtractError::Transform { .. })
    ));
    let scenes = extract_mesh_objects(&w, object, &Placer, units(1.0)).unwrap();
    assert_eq!(scenes[0].transform(), &placed_transform(0, 1));
}

#[test]
fn placed_templates_are_not_drawn_for_inactive_or_destroyed_objects() {
    let mut w = world();
    let object = drawn(&mut w, at(1.0, 0.0, 0.0));
    w.set_active(object, false).unwrap();
    let options = MeshExtractOptions::default();
    assert!(
        extract_mesh_objects(&w, object, &Placer, options)
            .unwrap()
            .is_empty()
    );
    w.set_active(object, true).unwrap();
    w.destroy_object(object).unwrap();
    assert!(
        extract_mesh_objects(&w, object, &Placer, options)
            .unwrap()
            .is_empty()
    );
}
