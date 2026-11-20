//! The environment lighting a world holds: what a scene or a script writes, what a read gives
//! and what the graphics snapshot carries to the host.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, PROBE_COEFFICIENTS, Reflection, ReflectionMode,
};
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{
    GraphicsError, OffscreenWork, TextureRef, WorldRenderTexture,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{World, WorldError};

mod graphics_world;
use graphics_world::{camera, render_texture, snapshot, world};

const CUBE: u64 = (1 << 40) + 3;
const BAKED: u64 = (1 << 40) + 4;

fn probe() -> [f32; PROBE_COEFFICIENTS] {
    std::array::from_fn(|index| 0.25 * f32::from(u8::try_from(index).unwrap()) - 1.5)
}

fn lit() -> Environment {
    Environment {
        ambient: Some(Ambient {
            mode: AmbientMode::Custom,
            sky: [0.1, 0.2, 0.3, 1.0],
            equator: [0.4, 0.5, 0.6, 1.0],
            ground: [0.7, 0.8, 0.9, 1.0],
            flat: [0.3, 0.2, 0.1, 1.0],
            intensity: 1.5,
            probe: probe(),
        }),
        reflection: Some(Reflection {
            mode: ReflectionMode::Custom,
            intensity: 0.5,
            custom: TextureRef::Asset(CUBE),
            skybox: TextureRef::Asset(BAKED),
        }),
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, found {other:?}"),
    }
}

/// A frame of work, so that the snapshot copies the world's state.
fn frame(world: &mut World) {
    let target = render_texture(world, [4, 4]);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
}

#[test]
fn a_new_world_holds_no_environment() {
    let world = world();
    assert_eq!(*world.environment(), Environment::default());
    assert!(world.environment().ambient.is_none());
    assert!(world.environment().reflection.is_none());
}

#[test]
fn the_environment_a_scene_wrote_reads_back_and_a_none_section_goes_back_to_unset() {
    let mut world = world();
    world.set_environment(lit()).unwrap();
    assert_eq!(*world.environment(), lit());
    let ambient_only = Environment {
        reflection: None,
        ..lit()
    };
    world.set_environment(ambient_only.clone()).unwrap();
    assert_eq!(*world.environment(), ambient_only);
}

#[test]
fn a_reflection_reads_the_custom_cube_when_the_mode_and_a_cube_say_so_and_the_skybox_cube_otherwise()
 {
    let mut reflection = lit().reflection.unwrap();
    assert_eq!(reflection.source(), &TextureRef::Asset(CUBE));
    reflection.mode = ReflectionMode::Skybox;
    assert_eq!(reflection.source(), &TextureRef::Asset(BAKED));
    reflection.mode = ReflectionMode::Custom;
    reflection.custom = TextureRef::None;
    assert_eq!(reflection.source(), &TextureRef::Asset(BAKED));
    reflection.skybox = TextureRef::None;
    assert_eq!(reflection.source(), &TextureRef::None);
}

#[test]
fn the_defaults_of_a_new_scene_are_the_values_a_section_nobody_wrote_reads_as() {
    let ambient = Ambient::default();
    assert_eq!(ambient.mode, AmbientMode::Skybox);
    assert_eq!(ambient.intensity, 1.0);
    assert_eq!(ambient.probe, [0.0; PROBE_COEFFICIENTS]);
    assert_eq!(ambient.flat, ambient.sky);
    let reflection = Reflection::default();
    assert_eq!(reflection.mode, ReflectionMode::Skybox);
    assert_eq!(reflection.intensity, 1.0);
    assert_eq!(reflection.source(), &TextureRef::None);
}

#[test]
fn a_value_that_is_not_finite_or_an_intensity_below_zero_is_refused_and_changes_nothing() {
    let mut world = world();
    world.set_environment(lit()).unwrap();
    let mut nan_probe = lit();
    nan_probe.ambient.as_mut().unwrap().probe[26] = f32::NAN;
    let mut infinite_colour = lit();
    infinite_colour.ambient.as_mut().unwrap().ground[0] = f32::INFINITY;
    let mut infinite_intensity = lit();
    infinite_intensity.reflection.as_mut().unwrap().intensity = f32::INFINITY;
    for refused in [nan_probe, infinite_colour, infinite_intensity] {
        assert_eq!(
            graphics_error(world.set_environment(refused)),
            GraphicsError::NotFinite
        );
    }
    let mut negative_ambient = lit();
    negative_ambient.ambient.as_mut().unwrap().intensity = -0.5;
    let mut negative_reflection = lit();
    negative_reflection.reflection.as_mut().unwrap().intensity = -0.5;
    for refused in [negative_ambient, negative_reflection] {
        assert_eq!(
            graphics_error(world.set_environment(refused)),
            GraphicsError::InvalidEnvironment
        );
    }
    assert_eq!(*world.environment(), lit());
}

#[test]
fn a_reflection_source_the_world_cannot_name_is_refused() {
    let mut world = world();
    let gone = render_texture(&mut world, [4, 4]);
    world.destroy_render_texture(gone).unwrap();
    for source in [
        TextureRef::Asset(0),
        TextureRef::Global("_Name".into()),
        TextureRef::Temporary(3),
        TextureRef::Render(gone),
    ] {
        let mut environment = lit();
        environment.reflection.as_mut().unwrap().custom = source;
        assert!(world.set_environment(environment).is_err());
    }
    assert_eq!(*world.environment(), Environment::default());
}

#[test]
fn a_snapshot_carries_the_environment_with_the_frames_state() {
    let mut world = world();
    world.set_environment(lit()).unwrap();
    frame(&mut world);
    let snapshot = snapshot(&mut world);
    assert_eq!(snapshot.environment, lit());
}

#[test]
fn a_render_texture_destroyed_after_the_environment_named_it_reads_as_no_texture() {
    let mut world = world();
    let texture = render_texture(&mut world, [4, 4]);
    let mut environment = lit();
    environment.reflection.as_mut().unwrap().custom = TextureRef::Render(texture);
    world.set_environment(environment).unwrap();
    world.destroy_render_texture(texture).unwrap();
    frame(&mut world);
    let held = snapshot(&mut world).environment.reflection.unwrap();
    assert_eq!(held.custom, TextureRef::None);
    assert_eq!(held.skybox, TextureRef::Asset(BAKED));
}

#[test]
fn the_environment_round_trips_through_the_byte_encoding() {
    let mut world = world();
    world.set_environment(lit()).unwrap();
    frame(&mut world);
    let snapshot = snapshot(&mut world);
    let decoded = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded.environment, lit());
    assert_eq!(decoded, snapshot);
    let sizes: WorldRenderTexture = *decoded.render_textures.values().next().unwrap();
    assert_eq!(sizes.width, 4);
}
