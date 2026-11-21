//! Generating the mip levels of render textures: the world's explicit record, and the textures
//! whose flag asks for it after they are written. The worlds are synthetic; the records and the
//! texels are predicted from the work listed.

use glam::{Quat, Vec3};
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture, WorldSampler,
    dimension,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentHandle, World};

use super::super::frame::{Adapted, Settings, adapt};
use super::super::ids;
use super::gpu::{Rig, SIDE, camera_render, shader, vector};
use super::support::{
    available, camera, compute_buffer, compute_shader, dispatch, render_texture, snapshot, world,
};

const RGBA8: u32 = 8;
const R32_FLOAT: u32 = 49;
const FAILURE: &str = "rendertexture:MipsUnsupported";

/// A render texture of `SIDE` by `SIDE` texels with `mips` levels whose flag is `auto`.
fn mipped(world: &mut World, (format, mips): (u32, u32), auto: bool) -> RenderTextureId {
    shaped(world, (format, mips, 0), (dimension::TEX_2D, 1), auto)
}

fn shaped(
    world: &mut World,
    (format, mips, depth_bits): (u32, u32, u32),
    (dimension, volume_depth): (u32, u32),
    auto: bool,
) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format,
            depth_bits,
            mip_count: mips,
            srgb: false,
            dimension,
            volume_depth,
            auto_generate_mips: auto,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

fn blit(world: &mut World, source: RenderTextureId, target: RenderTextureId) {
    record(
        world,
        OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(target),
            material: None,
            pass: 0,
        },
    );
}

fn camera_into(world: &mut World, target: RenderTextureId) -> ComponentHandle {
    let handle = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    record(
        world,
        OffscreenWork::CameraRender {
            camera: handle,
            target,
        },
    );
    handle
}

fn generate(world: &mut World, texture: RenderTextureId) {
    record(world, OffscreenWork::GenerateMips { texture });
}

fn clear(world: &mut World, target: RenderTextureId) {
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(target)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
    record(
        world,
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [0.25, 0.5, 0.75, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
}

fn adapted(world: &mut World) -> Adapted {
    let snapshot = snapshot(world);
    adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    )
}

/// The frame's records as words, with a texture named by the number a world gives it.
fn shape(adapted: &Adapted) -> Vec<String> {
    adapted
        .frame
        .records
        .iter()
        .map(|record| match record {
            Record::CameraRender { .. } => "camera".to_owned(),
            Record::Blit { .. } => "blit".to_owned(),
            Record::CopyTexture { .. } => "copy".to_owned(),
            Record::Clear { .. } => "clear".to_owned(),
            Record::Draws(_) => "draws".to_owned(),
            Record::Procedural(_) => "procedural".to_owned(),
            Record::Globals(_) => "globals".to_owned(),
            Record::GenerateMips { texture } => format!("mips {}", texture.0),
        })
        .collect()
}

fn mips_of(texture: RenderTextureId) -> String {
    format!("mips {}", ids::render_texture(texture).0)
}

#[test]
fn a_flagged_texture_a_camera_wrote_is_generated_before_the_blit_that_reads_it() {
    let mut world = world();
    let (a, b) = (
        mipped(&mut world, (RGBA8, 3), true),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    camera_into(&mut world, a);
    blit(&mut world, a, b);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["camera", &mips_of(a), "blit"]);
    assert_eq!(adapted.report.built, 3);
}

#[test]
fn consecutive_writes_to_a_flagged_texture_are_generated_once_before_the_next_read() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    let [b, c, d] = [0; 3].map(|_| render_texture(&mut world, [SIDE, SIDE]));
    blit(&mut world, b, a);
    blit(&mut world, c, a);
    blit(&mut world, a, d);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["blit", "blit", &mips_of(a), "blit"]);
}

#[test]
fn a_flagged_texture_written_again_after_a_read_is_generated_again() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    let [b, c] = [0; 2].map(|_| render_texture(&mut world, [SIDE, SIDE]));
    blit(&mut world, b, a);
    blit(&mut world, a, c);
    blit(&mut world, b, a);
    let adapted = adapted(&mut world);
    let mips = mips_of(a);
    assert_eq!(shape(&adapted), ["blit", &mips, "blit", "blit", &mips]);
}

#[test]
fn a_flagged_texture_nothing_reads_is_generated_at_the_end_of_the_frame() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    camera_into(&mut world, a);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["camera", &mips_of(a)]);
}

#[test]
fn a_read_of_another_texture_does_not_generate_the_flagged_one() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    let [b, c, d] = [0; 3].map(|_| render_texture(&mut world, [SIDE, SIDE]));
    camera_into(&mut world, a);
    blit(&mut world, b, c);
    blit(&mut world, a, d);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["camera", "blit", &mips_of(a), "blit"]);
}

#[test]
fn a_flagged_texture_that_is_the_target_of_a_clear_is_generated() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    clear(&mut world, a);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["clear", &mips_of(a)]);
}

#[test]
fn a_texture_that_is_not_flagged_or_has_one_level_or_was_not_written_is_not_generated() {
    let mut world = world();
    let unflagged = mipped(&mut world, (RGBA8, 3), false);
    let single = mipped(&mut world, (RGBA8, 1), true);
    let unwritten = mipped(&mut world, (RGBA8, 3), true);
    let source = render_texture(&mut world, [SIDE, SIDE]);
    blit(&mut world, source, unflagged);
    blit(&mut world, source, single);
    blit(&mut world, unwritten, source);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["blit", "blit", "blit"]);
    assert!(adapted.report.failures.is_empty());
}

#[test]
fn an_explicit_record_is_mapped_in_call_order_for_a_texture_that_is_not_flagged() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), false);
    let [b, c] = [0; 2].map(|_| render_texture(&mut world, [SIDE, SIDE]));
    blit(&mut world, b, a);
    generate(&mut world, a);
    blit(&mut world, a, c);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["blit", &mips_of(a), "blit"]);
    assert_eq!(adapted.report.built_by_kind["GenerateMips"], 1);
}

#[test]
fn an_explicit_record_stands_for_the_generation_a_flagged_texture_would_get() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    let b = render_texture(&mut world, [SIDE, SIDE]);
    blit(&mut world, b, a);
    generate(&mut world, a);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["blit", &mips_of(a)]);
}

#[test]
fn a_dispatch_that_binds_a_flagged_texture_is_placed_after_its_generation() {
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    camera_into(&mut world, a);
    let (shader, buffer) = (
        compute_shader(&mut world, "compute"),
        compute_buffer(&mut world, 4),
    );
    dispatch(&mut world, shader, (a, buffer), [1, 1, 1]);
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["camera", &mips_of(a)]);
    assert_eq!(adapted.placements.len(), 1);
    assert_eq!(adapted.placements[0].after, 2);
}

#[test]
fn a_flagged_texture_the_device_cannot_generate_is_counted_by_name_and_left_alone() {
    let mut world = world();
    let volume = shaped(&mut world, (RGBA8, 3, 0), (dimension::TEX_3D, 4), true);
    let depth = shaped(&mut world, (0, 3, 24), (dimension::TEX_2D, 1), true);
    let wide = mipped(&mut world, (R32_FLOAT, 3), true);
    for texture in [volume, depth, wide, wide] {
        camera_into(&mut world, texture);
    }
    let adapted = adapted(&mut world);
    assert_eq!(shape(&adapted), ["camera"; 4]);
    // Each texture is counted once, however often it is written.
    assert_eq!(adapted.report.failures[FAILURE], 3);
    let census = &adapted.report.census;
    assert_eq!(census["auto mips left out: 3D texture"], 1);
    assert_eq!(census["auto mips left out: depth-only texture"], 1);
    assert_eq!(census["auto mips left out: format 49 is not filtered"], 1);
}

#[test]
fn a_32_bit_float_texture_is_generated_when_the_device_filters_it() {
    let mut world = world();
    let wide = mipped(&mut world, (R32_FLOAT, 3), true);
    camera_into(&mut world, wide);
    let snapshot = snapshot(&mut world);
    let mut available = available(&snapshot, 1, &[]);
    available.float32_filterable = true;
    let adapted = adapt(&snapshot, &available, &Settings::default());
    assert_eq!(shape(&adapted), ["camera", &mips_of(wide)]);
    assert!(!adapted.report.failures.contains_key(FAILURE));
}

/// A blit of every pass of the synthetic `blit` shader from `source` to `target`.
fn shader_blit(world: &mut World, source: RenderTextureId, target: RenderTextureId) {
    let blit = shader(world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    record(
        world,
        OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(target),
            material: Some(material),
            pass: -1,
        },
    );
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// Level 1 of the texture a camera drew over its left half, as two by two texels: the left
/// column is the camera's red and the right the clear's blue.
fn assert_left_red_right_blue(bytes: &[u8]) {
    assert_eq!(bytes.len(), 2 * 2 * 4);
    for row in 0..2 {
        assert_eq!(bytes[row * 8..][..4], RED, "row {row}, column 0");
        assert_eq!(bytes[row * 8 + 4..][..4], BLUE, "row {row}, column 1");
    }
}

fn level_one(rig: &mut Rig, world: &mut World, texture: RenderTextureId) -> (Adapted, Vec<u8>) {
    let (adapted, mut operations, _) = rig.frame(world, &[]);
    let image = operations
        .read_back(ids::render_texture(texture), 1, 0)
        .unwrap();
    rig.run(operations);
    (adapted, image.bytes(&rig.device))
}

#[test]
fn a_blit_reads_every_level_of_a_flagged_texture_a_camera_wrote_and_level_one_is_the_average() {
    let mut rig = Rig::new();
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), true);
    let b = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, a);
    shader_blit(&mut world, a, b);
    let (adapted, level) = level_one(&mut rig, &mut world, a);
    assert!(
        !adapted
            .report
            .failures
            .contains_key("draw:SampledWrittenLevelsOnly"),
        "{:?}",
        adapted.report.failures
    );
    assert_left_red_right_blue(&level);
}

#[test]
fn a_blit_reads_only_the_written_level_of_an_unflagged_texture_with_no_record() {
    let mut rig = Rig::new();
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), false);
    let b = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, a);
    shader_blit(&mut world, a, b);
    let (adapted, _) = level_one(&mut rig, &mut world, a);
    assert_eq!(adapted.report.failures["draw:SampledWrittenLevelsOnly"], 3);
    assert!(
        !shape(&adapted).iter().any(|word| word.starts_with("mips")),
        "{:?}",
        shape(&adapted)
    );
}

#[test]
fn an_explicit_record_after_a_blit_into_an_unflagged_texture_lets_later_reads_bind_every_level() {
    let mut rig = Rig::new();
    let mut world = world();
    let a = mipped(&mut world, (RGBA8, 3), false);
    let [b, c] = [0; 2].map(|_| render_texture(&mut world, [SIDE, SIDE]));
    camera_render(&mut world, b);
    blit(&mut world, b, a);
    generate(&mut world, a);
    shader_blit(&mut world, a, c);
    let (adapted, level) = level_one(&mut rig, &mut world, a);
    assert!(
        !adapted
            .report
            .failures
            .contains_key("draw:SampledWrittenLevelsOnly"),
        "{:?}",
        adapted.report.failures
    );
    assert_left_red_right_blue(&level);
}

#[test]
fn flagged_textures_the_device_cannot_generate_do_not_fail_the_frame() {
    let mut rig = Rig::new();
    let mut world = world();
    let wide = mipped(&mut world, (R32_FLOAT, 3), true);
    let volume = shaped(&mut world, (RGBA8, 3, 0), (dimension::TEX_3D, 4), true);
    let a = mipped(&mut world, (RGBA8, 3), true);
    let b = render_texture(&mut world, [SIDE, SIDE]);
    clear(&mut world, wide);
    camera_into(&mut world, volume);
    camera_render(&mut world, a);
    shader_blit(&mut world, a, b);
    let (adapted, level) = level_one(&mut rig, &mut world, a);
    assert_eq!(adapted.report.failures[FAILURE], 2);
    assert!(
        !adapted
            .report
            .failures
            .contains_key("record:GenerateMips:MipsOfFormat")
    );
    assert!(
        !adapted
            .report
            .failures
            .contains_key("record:GenerateMips:MipsOfVolume")
    );
    assert_left_red_right_blue(&level);
}

#[test]
fn explicit_records_for_textures_that_cannot_be_generated_are_dropped_and_counted_by_name() {
    let mut rig = Rig::new();
    let mut world = world();
    // A 3D texture with a chain is itself refused by the state, which leaves out the record that
    // names it; a 32-bit float texture builds, and only its record is refused.
    let volume = shaped(&mut world, (RGBA8, 3, 0), (dimension::TEX_3D, 4), false);
    let wide = mipped(&mut world, (R32_FLOAT, 3), false);
    let a = mipped(&mut world, (RGBA8, 3), false);
    let b = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, a);
    generate(&mut world, volume);
    generate(&mut world, wide);
    generate(&mut world, a);
    shader_blit(&mut world, a, b);
    let (adapted, level) = level_one(&mut rig, &mut world, a);
    let failures = &adapted.report.failures;
    assert_eq!(
        failures.get("rendertexture:VolumeShape"),
        Some(&1),
        "{failures:?}"
    );
    assert_eq!(
        failures.get("record:UnusableRenderTexture"),
        Some(&1),
        "{failures:?}"
    );
    assert_eq!(
        failures.get("record:GenerateMips:MipsOfFormat"),
        Some(&1),
        "{failures:?}"
    );
    assert!(
        !failures.contains_key("draw:SampledWrittenLevelsOnly"),
        "{failures:?}"
    );
    assert_left_red_right_blue(&level);
}
