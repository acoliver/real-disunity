//! Which lights a frame lists and what each carries: every enabled light on an active object, in
//! component order, with its settings and its matrix in the frame's view basis.
use glam::Vec3;
use worldrunner_world_simulation::presentation::{
    Light, LightKind, LightSettings, LightShadowMode, light_views::extract_lights,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, World,
    WorldSettings,
};

fn new_world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn sun() -> Light {
    Light {
        enabled: true,
        settings: LightSettings {
            kind: LightKind::Directional,
            color: [1.0, 0.9, 0.8, 1.0],
            intensity: 1.5,
            shadows: LightShadowMode::Soft,
            shadow_bias: 0.1,
            ..LightSettings::default()
        },
    }
}

fn add_light(world: &mut World, light: Light, desc: ObjectDesc) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(desc).unwrap();
    let component = world
        .add_component(object, ComponentDesc::Light(light))
        .unwrap();
    (object, component)
}

fn listed(world: &World) -> Vec<ObjectHandle> {
    extract_lights(world)
        .unwrap()
        .into_iter()
        .map(|view| view.object)
        .collect()
}

#[test]
fn an_enabled_light_on_an_active_object_is_listed_with_its_settings() {
    let mut world = new_world();
    let (object, _) = add_light(&mut world, sun(), ObjectDesc::default());
    let lights = extract_lights(&world).unwrap();
    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].object, object);
    assert_eq!(lights[0].settings, sun().settings);
}

#[test]
fn a_disabled_light_or_one_on_an_inactive_object_is_left_out_until_restored() {
    let mut world = new_world();
    let (sun_object, sun_component) = add_light(&mut world, sun(), ObjectDesc::default());
    let (lamp, _) = add_light(&mut world, Light::default(), ObjectDesc::default());
    assert_eq!(listed(&world), vec![sun_object, lamp]);

    let disabled = Light {
        enabled: false,
        ..sun()
    };
    world
        .set_component(sun_component, ComponentDesc::Light(disabled))
        .unwrap();
    assert_eq!(listed(&world), vec![lamp]);
    world
        .set_component(sun_component, ComponentDesc::Light(sun()))
        .unwrap();

    world.set_active(sun_object, false).unwrap();
    assert_eq!(listed(&world), vec![lamp]);
    world.set_active(sun_object, true).unwrap();
    assert_eq!(listed(&world), vec![sun_object, lamp]);
}

#[test]
fn a_child_light_carries_its_parents_placement_in_the_view_basis() {
    let mut world = new_world();
    let parent = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(1.0, 2.0, 3.0),
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        })
        .unwrap();
    let child_desc = ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(0.0, 0.0, 4.0),
            ..LocalTransform::default()
        },
        ..ObjectDesc::default()
    };
    let (child, _) = add_light(&mut world, sun(), child_desc);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world.set_active(parent, false).unwrap();
    assert!(listed(&world).is_empty(), "a child of an inactive object");
    world.set_active(parent, true).unwrap();

    let matrix = extract_lights(&world).unwrap()[0].local_to_world;
    // World (1, 2, 7) in the view basis, where (x, y, z) becomes (x, -z, y).
    assert_eq!(&matrix[12..15], &[1.0, -7.0, 2.0]);
    assert_eq!(
        &matrix[8..11],
        &[0.0, -1.0, 0.0],
        "the light's forward axis"
    );
}
