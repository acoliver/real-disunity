//! A retained host shows the frames of a native 2D world over the clear colour they carry.
use super::*;
use glam::Vec3;
use std::{collections::BTreeMap, sync::Arc};
use worldrunner_render_scene::simulation::WorldSceneContribution;
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
    presentation::{Camera, Sprite, SpriteRenderer, extraction::extract_world},
};

const SIZE: [u32; 2] = [64, 64];

fn empty_scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

/// An orthographic camera over `background` that sees 10 units of height, and, with a `tint`, a
/// white sprite 4 units wide in the middle of the view.
fn contribution(
    size: [u32; 2],
    background: [f64; 4],
    tint: Option<[f32; 4]>,
) -> WorldSceneContribution {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let camera = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(
            camera,
            ComponentDesc::Camera(Camera {
                background,
                ..Camera::default()
            }),
        )
        .unwrap();
    if let Some(color) = tint {
        let square = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                square,
                ComponentDesc::Sprite(SpriteRenderer {
                    sprite: Some(Sprite {
                        identity: 0,
                        texture: 1,
                        rect: [0., 0., 8., 8.],
                        pivot: [0.5, 0.5],
                        pixels_per_unit: 2.,
                        texture_size: [8, 8],
                    }),
                    color,
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    let white = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 8,
        height: 8,
        mips: 1,
        bytes: vec![255; 8 * 8 * 4].into_boxed_slice(),
    };
    WorldSceneContribution::new(
        extract_world(&world, size).unwrap(),
        &BTreeMap::from([(1, white)]),
    )
    .unwrap()
}

fn show(run: &mut WindowlessRetained<'_>, contribution: WorldSceneContribution) {
    run.host_mut()
        .apply(RetainedUpdate::World(Arc::new(contribution)))
        .unwrap();
}

/// The colour at column `x` and row `y` counted from the top.
fn colour(image: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let at = ((y * image.viewport.width + x) * 4) as usize;
    [image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]]
}

fn assert_near(actual: [u8; 3], expected: [u8; 3]) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{actual:?} is not {expected:?}"
    );
}

#[test]
fn a_world_update_draws_its_sprites_over_its_clear_colour() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(empty_scene()).windowless(&mut driver, None).unwrap();
    show(
        &mut run,
        contribution(SIZE, [0., 0., 1., 1.], Some([1., 0.5, 0., 1.])),
    );
    let image = capture(&mut run, &events).1;
    // The sprite covers 4 of the view's 10 units, so the middle 26 pixels are orange and the
    // corners show the camera's blue.
    assert_near(colour(&image, 32, 32), [255, 188, 0]);
    assert_near(colour(&image, 24, 40), [255, 188, 0]);
    for (x, y) in [(2, 2), (61, 2), (2, 61), (61, 61)] {
        assert_near(colour(&image, x, y), [0, 0, 255]);
    }
    run.shutdown().unwrap();
}

#[test]
fn a_later_world_update_replaces_the_sprites_and_the_clear_colour() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(empty_scene()).windowless(&mut driver, None).unwrap();
    show(
        &mut run,
        contribution(SIZE, [0., 0., 1., 1.], Some([1., 0.5, 0., 1.])),
    );
    capture(&mut run, &events);
    show(&mut run, contribution(SIZE, [0., 1., 0., 1.], None));
    let image = capture(&mut run, &events).1;
    assert_near(colour(&image, 32, 32), [0, 255, 0]);
    assert_near(colour(&image, 2, 2), [0, 255, 0]);
    run.shutdown().unwrap();
}

#[test]
fn a_world_frame_for_another_size_is_refused() {
    let mut host = host(empty_scene());
    let generation = host.accepted_generation();
    let error = host
        .apply(RetainedUpdate::World(Arc::new(contribution(
            [32, 32],
            [0., 0., 0., 1.],
            None,
        ))))
        .unwrap_err();
    assert!(matches!(error, ViewerError::InvalidOptions(_)), "{error}");
    assert_eq!(host.accepted_generation(), generation);
}

#[test]
fn a_clear_colour_that_is_not_a_colour_is_refused() {
    let mut host = host(empty_scene());
    for clear in [
        [f64::NAN, 0., 0., 1.],
        [0., 1.5, 0., 1.],
        [0., 0., -0.5, 1.],
    ] {
        let mut frame = contribution(SIZE, [0., 0., 0., 1.], None);
        frame.clear = clear;
        let error = host
            .apply(RetainedUpdate::World(Arc::new(frame)))
            .unwrap_err();
        assert!(matches!(error, ViewerError::InvalidOptions(_)), "{error}");
    }
    assert_eq!(host.accepted_generation(), 0);
}

#[test]
fn a_host_with_directional_shadows_refuses_a_world_frame() {
    let mut host = host(empty_scene());
    host.options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    let error = host
        .apply(RetainedUpdate::World(Arc::new(contribution(
            SIZE,
            [0., 0., 0., 1.],
            None,
        ))))
        .unwrap_err();
    assert!(matches!(error, ViewerError::UnsupportedScene(_)), "{error}");
    assert_eq!(host.accepted_generation(), 0);
}
