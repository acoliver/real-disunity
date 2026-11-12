//! A camera's renderShadows setting: what the camera data accepts and what a frame reports for
//! the camera it presents.
use worldrunner_world_simulation::presentation::{
    Camera, UniversalAdditionalCameraData, camera_data::presented_render_shadows,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ObjectDesc, ObjectHandle, World, WorldError, WorldSettings,
};

/// renderShadows is property 5 of the camera data.
const RENDER_SHADOWS: u32 = 5;

fn new_world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn camera_data(render_shadows: u32) -> UniversalAdditionalCameraData {
    let mut data = UniversalAdditionalCameraData::default();
    data.set(RENDER_SHADOWS, render_shadows).unwrap();
    data
}

/// A camera object with `depth`, and its camera data when `data` is some.
fn add_camera(
    world: &mut World,
    depth: i32,
    data: Option<UniversalAdditionalCameraData>,
) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let camera = Camera {
        depth,
        ..Camera::default()
    };
    world
        .add_component(object, ComponentDesc::Camera(camera))
        .unwrap();
    if let Some(data) = data {
        world
            .add_component(object, ComponentDesc::UniversalAdditionalCameraData(data))
            .unwrap();
    }
    object
}

#[test]
fn render_shadows_is_off_by_default_and_takes_on_and_off_only() {
    let mut data = UniversalAdditionalCameraData::default();
    assert_eq!(data.get(RENDER_SHADOWS), Some(0));
    data.set(RENDER_SHADOWS, 1).unwrap();
    assert_eq!(data.get(RENDER_SHADOWS), Some(1));
    data.set(RENDER_SHADOWS, 0).unwrap();
    assert_eq!(data.get(RENDER_SHADOWS), Some(0));
    assert_eq!(
        data.set(RENDER_SHADOWS, 2),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("renderShadows")
        ))
    );
    assert_eq!(
        data.get(RENDER_SHADOWS),
        Some(0),
        "a refused value changes nothing"
    );
}

#[test]
fn a_frame_reports_the_render_shadows_of_the_camera_it_presents() {
    let mut world = new_world();
    add_camera(&mut world, 0, Some(camera_data(0)));
    assert_eq!(presented_render_shadows(&world).unwrap(), Some(false));
    let front = add_camera(&mut world, 1, Some(camera_data(1)));
    assert_eq!(presented_render_shadows(&world).unwrap(), Some(true));
    world.set_active(front, false).unwrap();
    assert_eq!(
        presented_render_shadows(&world).unwrap(),
        Some(false),
        "the inactive front camera is not presented"
    );
}

#[test]
fn no_presented_camera_or_one_without_camera_data_reports_nothing() {
    let mut world = new_world();
    assert_eq!(presented_render_shadows(&world).unwrap(), None);
    add_camera(&mut world, 0, None);
    assert_eq!(presented_render_shadows(&world).unwrap(), None);
}
