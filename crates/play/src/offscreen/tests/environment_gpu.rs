//! The scene's environment lighting on a real adapter (lavapipe in CI): the reflection cube a
//! draw samples as `unity_SpecCube0` and the ambient light it reads as `unity_SHAr`.
//!
//! The cube pass reads the direction its column of a six-column target names (+X, -X, +Y, -Y, +Z,
//! -Z), so every expected texel is the colour the test stored for that face. The ambient pass
//! writes the fourth component of `unity_SHAr` as grey. Every expected value comes from the
//! environment the test gives the world, not from the output.

use std::collections::BTreeMap;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, Reflection, ReflectionMode,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef,
};

use super::ambient_gpu::{assert_texels, camera_render, flush_and_read};
use super::cube_volume_gpu::{BASE, TOP, columns, cube};
use super::flush::host;
use super::gpu::{Quad, SIDE, shader};
use super::lights_gpu::{Commands, filled, record};
use super::support::{
    material, render_texture, set_global_in_earlier_take, snapshot, texture_slot, world,
};
use crate::offscreen::assets::{AssetError, Assets};
use crate::offscreen::{ReflectionCube, Report, SPEC_CUBE0};

/// The identities the managed engine stored two cubes under, and a third in BC6H.
const CUBE: u64 = 1 << 40;
const OTHER: u64 = (1 << 40) + 1;
const BAKED: u64 = (1 << 40) + 10;
/// The ten-bit BC6H endpoint that decodes to the half float 1.0.
const BC6H_ONE: u16 = 495;
/// The read of a grey cube, 128 of 255 in each colour channel.
const GREY: f32 = 128.0 / 255.0;

/// The quad's mesh and the stored cubes.
struct Cubes(Vec<(u64, PlayerTexture)>);

impl Assets for Cubes {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(self
            .0
            .iter()
            .find(|(held, stored)| *held == identity && stored.generation != known)
            .map(|(_, stored)| stored.clone()))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// One BC6H block of the single-subset mode with ten-bit endpoints that holds `colour` in every
/// texel: both endpoints are the colour and every index is zero. A channel of 255 is the half
/// float 1.0 and a channel of 0 is 0.
fn bc6h_block(colour: [u8; 4]) -> [u8; 16] {
    let mut bits = 3_u128;
    for slot in 0..6 {
        let endpoint = if colour[slot % 3] == 0 { 0 } else { BC6H_ONE };
        bits |= u128::from(endpoint) << (5 + slot * 10);
    }
    bits.to_le_bytes()
}

/// A BC6H cube of 8x8 faces with four levels as the player stores a baked sky: level 0 of face
/// `f` is `BASE[f]` in its four blocks, and the three smaller levels are `TOP[f]`.
fn baked_cube() -> PlayerTexture {
    let bytes = (0..6)
        .flat_map(|face| {
            let level = |colour, blocks: usize| bc6h_block(colour).repeat(blocks);
            [level(BASE[face], 4), level(TOP[face], 3)].concat()
        })
        .collect();
    PlayerTexture {
        format: 24,
        linear: true,
        graphics_format: 106,
        ..cube(8, 4, bytes)
    }
}

/// The cube `CUBE` has the six colours of `BASE` as 1x1 faces, `OTHER` those of `TOP` and `BAKED`
/// is a sky baked in BC6H.
fn stored() -> Cubes {
    Cubes(vec![
        (CUBE, cube(1, 1, BASE.concat())),
        (OTHER, cube(1, 1, TOP.concat())),
        (BAKED, baked_cube()),
    ])
}

fn reflection(mode: ReflectionMode, custom: u64, skybox: u64) -> Environment {
    let source = |identity: u64| match identity {
        0 => TextureRef::None,
        identity => TextureRef::Asset(identity),
    };
    Environment {
        ambient: None,
        reflection: Some(Reflection {
            mode,
            intensity: 1.0,
            custom: source(custom),
            skybox: source(skybox),
        }),
    }
}

/// A world that blits a six by four target with the pass that reads `unity_SpecCube0` along the
/// direction of each column, and the target.
fn cube_world(before: Commands<'_>) -> (World, RenderTextureId) {
    let mut world = world();
    before(&mut world);
    let pass = shader(&mut world, "speccube");
    let blit = material(&mut world, pass);
    let source = render_texture(&mut world, [6, SIDE]);
    let out = render_texture(&mut world, [6, SIDE]);
    record(
        &mut world,
        OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(out),
            material: Some(blit),
            pass: 0,
        },
    );
    (world, out)
}

/// The target after a flush over the stored cubes, and the report.
fn read(mut world: World, out: RenderTextureId) -> (Vec<f32>, Report) {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let request = world.request_texture_pixels(out, 0, 0, 6, SIDE).unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &stored()).unwrap();
    outcome.apply(&mut world).unwrap();
    (world.take_texture_pixels(request).unwrap(), outcome.report)
}

fn with_environment(environment: Environment) -> (Vec<f32>, Report) {
    let (mut world, out) = cube_world(&|_| {});
    world.set_environment(environment).unwrap();
    let (pixels, report) = read(world, out);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    (pixels, report)
}

/// Whether the report counts one flush that read the cube `cube` names.
fn counts(report: &Report, cube: ReflectionCube) -> bool {
    report.census.get(cube.census()) == Some(&1)
}

fn grey() -> Vec<f32> {
    (0..6 * SIDE)
        .flat_map(|_| [GREY, GREY, GREY, 1.0])
        .collect()
}

fn assert_grey(found: &[f32]) {
    assert_texels(found, &grey());
}

fn counted_as_undeclared(report: &Report) -> BTreeMap<String, usize> {
    report
        .stand_ins
        .by_property
        .iter()
        .filter(|(name, _)| name.ends_with(" undeclared"))
        .map(|(name, count)| (name.clone(), *count))
        .collect()
}

fn undeclared_key(property: &str) -> String {
    format!("{property} undeclared")
}

#[test]
fn a_draw_reads_the_custom_cube_of_the_scene_as_unity_speccube0() {
    let (read, report) = with_environment(reflection(ReflectionMode::Custom, CUBE, OTHER));
    assert_eq!(read, columns(SIDE, &BASE));
    assert_eq!(counted_as_undeclared(&report), BTreeMap::new());
    assert_eq!(report.stand_ins.draws, 0, "{:?}", report.stand_ins);
    assert!(
        counts(&report, ReflectionCube::Bound),
        "{:?}",
        report.census
    );
}

#[test]
fn the_skybox_mode_reads_the_cube_baked_from_the_skybox_and_not_the_custom_one() {
    let (read, report) = with_environment(reflection(ReflectionMode::Skybox, CUBE, OTHER));
    assert_eq!(read, columns(SIDE, &TOP));
    assert_eq!(counted_as_undeclared(&report), BTreeMap::new());
}

#[test]
fn a_bc6h_cube_baked_from_the_skybox_is_what_a_draw_reads_as_unity_speccube0() {
    let (read, report) = with_environment(reflection(ReflectionMode::Skybox, 0, BAKED));
    assert_eq!(read, columns(SIDE, &BASE));
    assert_eq!(counted_as_undeclared(&report), BTreeMap::new());
    assert_eq!(report.stand_ins.draws, 0, "{:?}", report.stand_ins);
    assert!(
        counts(&report, ReflectionCube::Bound),
        "{:?}",
        report.census
    );
}

#[test]
fn a_custom_mode_without_a_custom_cube_reads_the_cube_baked_from_the_skybox() {
    let (read, _) = with_environment(reflection(ReflectionMode::Custom, 0, OTHER));
    assert_eq!(read, columns(SIDE, &TOP));
}

#[test]
fn a_scene_without_a_reflection_cube_reads_the_grey_cube_and_is_counted() {
    for (environment, cube) in [
        (Environment::default(), ReflectionCube::NotWritten),
        (
            reflection(ReflectionMode::Custom, 0, 0),
            ReflectionCube::NoCube,
        ),
    ] {
        let (read, report) = with_environment(environment);
        assert_grey(&read);
        assert!(counts(&report, cube), "{:?}", report.census);
        assert!(
            report
                .stand_ins
                .by_property
                .contains_key(&undeclared_key(SPEC_CUBE0)),
            "{:?}",
            report.stand_ins
        );
    }
}

#[test]
fn a_cube_the_host_does_not_hold_reads_the_grey_cube_and_is_counted() {
    let (mut world, out) = cube_world(&|_| {});
    let unstored = reflection(ReflectionMode::Custom, OTHER + 1, 0);
    world.set_environment(unstored).unwrap();
    let (read, report) = read(world, out);
    assert_grey(&read);
    assert_eq!(
        report.stand_ins.by_property,
        BTreeMap::from([(format!("{SPEC_CUBE0} exists-not-loaded"), 1)])
    );
    assert_eq!(report.failures.get("texture:NotStored"), Some(&1));
    assert!(
        counts(&report, ReflectionCube::Unreadable),
        "{:?}",
        report.census
    );
}

fn other_cube() -> PropertyValue {
    PropertyValue::Texture(SPEC_CUBE0.into(), texture_slot(TextureRef::Asset(OTHER)))
}

#[test]
fn a_cube_the_world_sets_as_a_global_replaces_the_scenes() {
    // Written in an earlier take, the cube is in the state the frame starts from.
    let (mut world, out) = cube_world(&|world| set_global_in_earlier_take(world, other_cube()));
    world
        .set_environment(reflection(ReflectionMode::Custom, CUBE, 0))
        .unwrap();
    let (pixels, report) = read(world, out);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(pixels, columns(SIDE, &TOP));
}

#[test]
fn a_cube_a_command_buffer_sets_replaces_the_scenes_from_where_it_is_recorded() {
    let mut world = world();
    let pass = shader(&mut world, "speccube");
    let blit = material(&mut world, pass);
    let source = render_texture(&mut world, [6, SIDE]);
    let (before, after) = (
        render_texture(&mut world, [6, SIDE]),
        render_texture(&mut world, [6, SIDE]),
    );
    let draw = |world: &mut World, out: RenderTextureId| {
        record(
            world,
            OffscreenWork::Blit {
                source: TextureRef::Render(source),
                target: TargetRef::Render(out),
                material: Some(blit),
                pass: 0,
            },
        );
    };
    world
        .set_environment(reflection(ReflectionMode::Custom, CUBE, 0))
        .unwrap();
    draw(&mut world, before);
    record(&mut world, OffscreenWork::SetGlobal(other_cube()));
    draw(&mut world, after);
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let requests = [before, after].map(|texture| {
        world
            .request_texture_pixels(texture, 0, 0, 6, SIDE)
            .unwrap()
    });
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &stored()).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(outcome.report.failures.is_empty());
    let [first, second] = requests.map(|request| world.take_texture_pixels(request).unwrap());
    assert_eq!(first, columns(SIDE, &BASE));
    assert_eq!(second, columns(SIDE, &TOP));
}

/// The environment whose flat ambient light is `colour` at `intensity`.
fn flat(colour: f32, intensity: f32) -> Environment {
    Environment {
        ambient: Some(Ambient {
            mode: AmbientMode::Flat,
            flat: [colour, colour, colour, 1.0],
            intensity,
            ..Ambient::default()
        }),
        reflection: None,
    }
}

#[test]
fn a_draw_reads_the_ambient_light_of_the_scene_instead_of_the_default() {
    let (mut world, drawn) = camera_render(&|_| {});
    world.set_environment(flat(0.5, 1.0)).unwrap();
    let (read, report) = flush_and_read(world, drawn);
    assert_texels(&read, &filled([0.5; 3]));
    assert!(!report.unset_constants.contains(crate::offscreen::SH_AR));
}

#[test]
fn the_ambient_intensity_scales_the_ambient_light() {
    let (mut world, drawn) = camera_render(&|_| {});
    world.set_environment(flat(0.25, 2.0)).unwrap();
    assert_texels(&flush_and_read(world, drawn).0, &filled([0.5; 3]));
}

#[test]
fn the_ambient_probe_of_the_scene_reaches_the_constant_term_of_the_draw() {
    // The scene stores the coefficients of the polynomial the shader evaluates, so the constant
    // term of red is the red the draw reads.
    let mut probe = [0.0; 27];
    probe[0] = 1.0;
    let environment = Environment {
        ambient: Some(Ambient {
            mode: AmbientMode::Skybox,
            probe,
            ..Ambient::default()
        }),
        reflection: None,
    };
    let (mut world, drawn) = camera_render(&|_| {});
    world.set_environment(environment).unwrap();
    let (read, _) = flush_and_read(world, drawn);
    // Only the red probe is lit, so the constant term of `unity_SHAr` is read.
    assert_texels(&read, &filled([1.0; 3]));
}

#[test]
fn a_scene_that_set_no_ambient_keeps_the_default() {
    let (mut world, drawn) = camera_render(&|_| {});
    world
        .set_environment(reflection(ReflectionMode::Custom, 0, 0))
        .unwrap();
    assert_texels(
        &flush_and_read(world, drawn).0,
        &filled([super::ambient_gpu::DEFAULT; 3]),
    );
}
