//! The buffers a camera draws into when a script sets them: the camera's own settings, the
//! camera setup that carries them, the snapshot that copies them and the bytes that carry that.
use glam::{Quat, Vec3};
use worldrunner_camera::CameraLens;
use worldrunner_world_simulation::graphics::snapshot_wire::{
    decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    CameraBuffers, GraphicsError, GraphicsSnapshot, OffscreenWork, RenderTextureId, TargetRef,
    WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::presentation::extraction::extract_world;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, ObjectDesc, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn snapshot(world: &mut World) -> GraphicsSnapshot {
    GraphicsSnapshot::take(world).unwrap()
}

fn render_texture(world: &mut World, [width, height]: [u32; 2]) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width,
            height,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn camera(
    world: &mut World,
    settings: Camera,
    _position: Vec3,
    _rotation: Quat,
) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Camera(settings))
        .unwrap()
}

fn eight(world: &mut World) -> Vec<RenderTextureId> {
    (0..8).map(|_| render_texture(world, [8, 8])).collect()
}

fn setup(
    camera: ComponentHandle,
    target: TargetRef,
    buffers: Option<CameraBuffers>,
) -> OffscreenWork {
    OffscreenWork::SetupCamera {
        camera,
        view: std::array::from_fn(|index| if index % 5 == 0 { 1.0 } else { 0.0 }),
        projection: std::array::from_fn(|index| if index % 5 == 0 { 1.0 } else { 0.0 }),
        target,
        buffers,
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result.unwrap_err() {
        WorldError::Graphics(error) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn buffers_hold_one_to_eight_different_colors_in_order_and_an_optional_depth() {
    let mut world = world();
    let ids = eight(&mut world);
    let buffers = CameraBuffers::new(&ids[..3], Some(ids[7])).unwrap();
    assert_eq!(buffers.colors(), &ids[..3]);
    assert_eq!(buffers.depth(), Some(ids[7]));
    assert!(buffers.names(ids[2]) && buffers.names(ids[7]) && !buffers.names(ids[5]));
    assert_eq!(CameraBuffers::new(&ids, None).unwrap().colors(), &ids[..]);
    for refused in [&[][..], &[ids[0], ids[1], ids[0]][..]] {
        assert_eq!(
            CameraBuffers::new(refused, None).unwrap_err(),
            GraphicsError::InvalidCameraBuffers
        );
    }
    let nine: Vec<_> = ids
        .iter()
        .copied()
        .chain([render_texture(&mut world, [8, 8])])
        .collect();
    assert_eq!(
        CameraBuffers::new(&nine, None).unwrap_err(),
        GraphicsError::InvalidCameraBuffers
    );
}

#[test]
fn a_camera_with_buffers_is_not_the_screen_camera() {
    let mut world = world();
    let ids = eight(&mut world);
    let screen = camera(
        &mut world,
        Camera {
            half_height: 3.,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let baking = camera(
        &mut world,
        Camera {
            depth: 10,
            half_height: 9.,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let lens = |world: &World| {
        extract_world(world, [64, 32])
            .unwrap()
            .camera_state
            .expect("a presented camera")
            .lens
    };
    let half_height = |world: &World| match lens(world) {
        CameraLens::Orthographic {
            half_height_meters, ..
        } => half_height_meters,
        other @ CameraLens::Perspective { .. } => panic!("an orthographic lens, not {other:?}"),
    };
    assert!(
        (half_height(&world) - 9.).abs() < 1e-9,
        "the higher depth wins"
    );
    let buffers = CameraBuffers::new(&ids[..2], None).unwrap();
    world.set_camera_buffers(baking, Some(buffers)).unwrap();
    assert!(
        (half_height(&world) - 3.).abs() < 1e-9,
        "buffers take it off the screen"
    );
    world.set_camera_buffers(baking, None).unwrap();
    assert!(
        (half_height(&world) - 9.).abs() < 1e-9,
        "without them it is back"
    );
    let _ = screen;
}

#[test]
fn setting_buffers_replaces_the_target_texture_and_refuses_what_it_cannot_hold() {
    let mut world = world();
    let ids = eight(&mut world);
    let target = render_texture(&mut world, [8, 8]);
    let handle = camera(
        &mut world,
        Camera {
            target_texture: Some(target),
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let target_texture = |world: &World| match world.component(handle).unwrap().desc {
        ComponentDesc::Camera(camera) => camera.target_texture,
        _ => panic!("a camera"),
    };
    assert_eq!(world.camera_buffers(handle), None);
    let buffers = CameraBuffers::new(&ids[..3], Some(ids[3])).unwrap();
    world.set_camera_buffers(handle, Some(buffers)).unwrap();
    assert_eq!(world.camera_buffers(handle), Some(buffers));
    assert_eq!(
        target_texture(&world),
        None,
        "the buffers replace the target texture"
    );
    // A texture that is gone refuses the whole set and leaves the buffers it had.
    world.destroy_render_texture(ids[5]).unwrap();
    let stale = CameraBuffers::new(&[ids[0], ids[5]], None).unwrap();
    assert!(world.set_camera_buffers(handle, Some(stale)).is_err());
    assert_eq!(world.camera_buffers(handle), Some(buffers));
    // Only a camera has buffers.
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let listener = world
        .add_component(object, ComponentDesc::AudioListener { enabled: true })
        .unwrap();
    assert!(world.set_camera_buffers(listener, Some(buffers)).is_err());
    world.set_camera_buffers(handle, None).unwrap();
    assert_eq!(world.camera_buffers(handle), None);
}

#[test]
fn a_setup_with_buffers_names_the_camera_target_and_live_textures() {
    let mut world = world();
    let ids = eight(&mut world);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let buffers = CameraBuffers::new(&ids[..3], Some(ids[3])).unwrap();
    world
        .record_offscreen(setup(camera, TargetRef::CameraTarget, Some(buffers)))
        .unwrap();
    // The buffers are the camera's own target, so no other target goes with them.
    assert_eq!(
        graphics_error(world.record_offscreen(setup(
            camera,
            TargetRef::Render(ids[4]),
            Some(buffers)
        ))),
        GraphicsError::InvalidCameraBuffers
    );
    world.destroy_render_texture(ids[1]).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(setup(
            camera,
            TargetRef::CameraTarget,
            Some(buffers)
        ))),
        GraphicsError::StaleRenderTexture
    );
    assert_eq!(
        world.offscreen_work().len(),
        1,
        "only the valid record is kept"
    );
}

#[test]
fn the_snapshot_carries_every_buffer_of_a_setup_and_of_the_camera() {
    let mut world = world();
    let ids = eight(&mut world);
    let buffers = CameraBuffers::new(&ids[..3], Some(ids[3])).unwrap();
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world.set_camera_buffers(camera, Some(buffers)).unwrap();
    world
        .record_offscreen(setup(camera, TargetRef::CameraTarget, Some(buffers)))
        .unwrap();
    let taken = snapshot(&mut world);
    let [
        OffscreenWork::SetupCamera {
            buffers: recorded, ..
        },
    ] = taken.work.as_slice()
    else {
        panic!("one camera setup: {:?}", taken.work);
    };
    assert_eq!(*recorded, Some(buffers));
    assert_eq!(taken.cameras[&camera].target_buffers, Some(buffers));
    for id in &ids[..4] {
        assert!(
            taken.render_textures.contains_key(id),
            "{id:?} is in the snapshot"
        );
    }
}

#[test]
fn a_buffer_destroyed_after_a_setup_named_it_stays_in_that_frames_snapshot() {
    let mut world = world();
    let ids = eight(&mut world);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let buffers = CameraBuffers::new(&ids[..2], Some(ids[2])).unwrap();
    world
        .record_offscreen(setup(camera, TargetRef::CameraTarget, Some(buffers)))
        .unwrap();
    world.destroy_render_texture(ids[1]).unwrap();
    world.destroy_render_texture(ids[2]).unwrap();
    let taken = snapshot(&mut world);
    assert!(taken.render_textures.contains_key(&ids[1]), "a color");
    assert!(taken.render_textures.contains_key(&ids[2]), "the depth");
}

#[test]
fn the_buffers_survive_the_bytes_that_carry_a_snapshot_and_a_record() {
    let mut world = world();
    let ids = eight(&mut world);
    let buffers = CameraBuffers::new(&ids, Some(ids[0])).unwrap();
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world.set_camera_buffers(camera, Some(buffers)).unwrap();
    world
        .record_offscreen(setup(camera, TargetRef::CameraTarget, Some(buffers)))
        .unwrap();
    let taken = snapshot(&mut world);
    let back = decode(&encode(&taken).unwrap()).unwrap();
    assert_eq!(back, taken);
    assert_eq!(back.cameras[&camera].target_buffers, Some(buffers));
    let record = taken.work[0].clone();
    assert_eq!(decode_work(&encode_work(&record).unwrap()).unwrap(), record);
}

#[test]
fn a_setup_without_buffers_records_none_whatever_the_camera_holds() {
    let mut world = world();
    let ids = eight(&mut world);
    let buffers = CameraBuffers::new(&ids[..2], None).unwrap();
    let handle = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world.set_camera_buffers(handle, Some(buffers)).unwrap();
    world
        .record_offscreen(setup(handle, TargetRef::CameraTarget, None))
        .unwrap();
    let taken = snapshot(&mut world);
    let [
        OffscreenWork::SetupCamera {
            buffers: recorded, ..
        },
    ] = taken.work.as_slice()
    else {
        panic!("one camera setup");
    };
    assert_eq!(*recorded, None);
    assert_eq!(taken.cameras[&handle].target_buffers, Some(buffers));
}
