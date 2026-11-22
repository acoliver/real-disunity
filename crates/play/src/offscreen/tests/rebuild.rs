//! The frame the player builds after the state refuses a record. The refused records are left
//! out, the rest of the frame runs as it would have without them, and the builds that find them
//! write nothing the frame does not submit. The worlds are synthetic and run on a real adapter
//! (lavapipe in CI). A copy between textures of different sizes is the refused record.

use std::collections::BTreeMap;

use worldrunner_render_program::unity::offscreen::frame::ReadRange;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::GraphicsSnapshot;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture, WorldSampler,
    dimension,
};

use super::super::adapter::{Settling, build};
use super::super::frame::Adapted;
use super::super::ids;
use super::gpu::{Quad, Rig, SIDE, camera_render, shader, vector};
use super::stand_ins::{camera_scene, gamma_rig};
use super::support::{render_texture, world};

const RGBA8: u32 = 8;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
/// What a frame that samples a render texture no build wrote counts that draw as.
const UNWRITTEN: &str = "_Albedo unwritten-render-texture";

fn copy(world: &mut World, source: RenderTextureId, target: RenderTextureId) {
    world
        .record_offscreen(OffscreenWork::CopyTexture {
            source: TextureRef::Render(source),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(target),
            target_element: 0,
            target_mip: 0,
        })
        .unwrap();
}

/// A render texture of `SIDE` by `SIDE` texels with `mips` levels, flagged for generation or not.
fn mipped(world: &mut World, mips: u32, auto: bool) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format: RGBA8,
            depth_bits: 0,
            mip_count: mips,
            srgb: false,
            dimension: dimension::TEX_2D,
            volume_depth: 1,
            auto_generate_mips: auto,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn failures(adapted: &Adapted) -> BTreeMap<&str, usize> {
    adapted
        .report
        .failures
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect()
}

/// The texel at column `x` of the first row of a `SIDE` wide image.
fn texel(bytes: &[u8], x: usize) -> [u8; 4] {
    bytes[x * 4..][..4].try_into().unwrap()
}

/// What the camera leaves: its red over the two left columns, the clear's blue over the rest.
fn assert_camera_image(bytes: &[u8]) {
    assert_eq!(texel(bytes, 0), RED);
    assert_eq!(texel(bytes, 1), RED);
    assert_eq!(texel(bytes, 2), BLUE);
    assert_eq!(texel(bytes, 3), BLUE);
}

/// A camera render and `LINKS` copies, each from the texture before it into a new one, with a copy
/// that cannot be made after the links `refused_after` names. Returns the report of the frame and
/// the last texture's image.
fn chain(refused_after: &[usize]) -> (Adapted, Vec<u8>) {
    const LINKS: usize = 20;
    let mut rig = Rig::new();
    let mut world = world();
    let first = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, first);
    let mut last = first;
    for link in 0..LINKS {
        let next = render_texture(&mut world, [SIDE, SIDE]);
        copy(&mut world, last, next);
        if refused_after.contains(&link) {
            // Four texels by four cannot be copied into eight by eight.
            let odd = render_texture(&mut world, [2 * SIDE, 2 * SIDE]);
            copy(&mut world, last, odd);
        }
        last = next;
    }
    let (adapted, operations, reads) = rig.frame(&mut world, &[last]);
    assert_eq!(
        adapted.report.built,
        1 + LINKS,
        "the camera and every kept copy"
    );
    assert_eq!(adapted.frame.records.len(), 1 + LINKS);
    rig.run(operations);
    let image = reads.into_iter().next().unwrap().bytes(&rig.device);
    (adapted, image)
}

#[test]
fn one_refused_record_among_many_costs_one_build_alone_and_leaves_the_rest_of_the_chain_intact() {
    let (adapted, image) = chain(&[10]);
    assert_eq!(
        failures(&adapted),
        BTreeMap::from([("record:CopyTexture:CopyMismatch", 1)])
    );
    // The refused record is built alone once. Building every render texture and every record
    // alone took 44 builds for this frame.
    let phases = adapted.report.phases;
    assert_eq!((phases.refusals, phases.probes), (1, 1), "{phases:?}");
    assert_camera_image(&image);
}

#[test]
fn each_of_several_refused_records_costs_one_build_alone() {
    let (adapted, image) = chain(&[3, 11, 19]);
    assert_eq!(
        failures(&adapted),
        BTreeMap::from([("record:CopyTexture:CopyMismatch", 3)])
    );
    let phases = adapted.report.phases;
    assert_eq!((phases.refusals, phases.probes), (1, 3), "{phases:?}");
    assert_camera_image(&image);
}

#[test]
fn a_texture_only_a_refused_record_would_write_stays_unwritten_for_the_next_frame() {
    let mut rig = gamma_rig();
    let mut world = world();
    let source = render_texture(&mut world, [SIDE, SIDE]);
    let odd = render_texture(&mut world, [2 * SIDE, 2 * SIDE]);
    let other = render_texture(&mut world, [SIDE, SIDE]);
    copy(&mut world, source, other);
    copy(&mut world, source, odd);
    let (adapted, _operations, _) = rig.frame(&mut world, &[]);
    assert_eq!(
        failures(&adapted),
        BTreeMap::from([("record:CopyTexture:CopyMismatch", 1)])
    );

    camera_scene(&mut world, "default_white", Some(TextureRef::Render(odd)));
    let (adapted, _operations, _) = rig.frame(&mut world, &[]);
    assert_eq!(
        adapted.report.stand_ins.by_property.get(UNWRITTEN),
        Some(&1),
        "{:?}",
        adapted.report.stand_ins
    );
}

/// The frame of a camera render into the new render texture, adapted, with a read of a mip level
/// the texture does not have. No part of the frame is refused: only the frame as a whole is,
/// by the read, which the builds of its parts do not see.
fn frame_refused_whole(rig: &mut Rig, world: &mut World) -> (Adapted, RenderTextureId) {
    let written = camera_scene(world, "camera", None);
    let snapshot = GraphicsSnapshot::take(world).unwrap();
    let mut adapted = rig.adapter.adapt(
        &snapshot,
        &Quad,
        (&mut rig.state, &mut rig.runtime),
        (false, &std::collections::BTreeMap::new()),
        Settling::Wait,
    );
    adapted.frame.later_reads.push(ReadRange {
        texture: ids::render_texture(written),
        mips: 5..6,
        layers: 0..1,
    });
    (adapted, written)
}

#[test]
fn a_frame_the_state_refuses_in_the_end_marks_no_render_texture_written() {
    let mut rig = gamma_rig();
    let mut world = world();
    let (mut adapted, written) = frame_refused_whole(&mut rig, &mut world);
    let refused = build(&mut rig.state, &mut rig.runtime, &rig.arena, &mut adapted);
    assert!(refused.is_err(), "the read of a level the texture lacks");
    assert_eq!(failures(&adapted), BTreeMap::new(), "no part is at fault");

    // Nothing of the refused frame was submitted, so no draw may read what it would have written.
    camera_scene(
        &mut world,
        "default_white",
        Some(TextureRef::Render(written)),
    );
    let (adapted, _operations, _) = rig.frame(&mut world, &[]);
    assert_eq!(
        adapted.report.stand_ins.by_property.get(UNWRITTEN),
        Some(&1),
        "{:?}",
        adapted.report.stand_ins
    );
}

#[test]
fn a_refused_record_and_a_frame_refused_in_the_end_leave_the_same_failures_and_no_writes() {
    let mut rig = gamma_rig();
    let mut world = world();
    let source = render_texture(&mut world, [SIDE, SIDE]);
    let odd = render_texture(&mut world, [2 * SIDE, 2 * SIDE]);
    copy(&mut world, source, odd);
    let (mut adapted, written) = frame_refused_whole(&mut rig, &mut world);
    let refused = build(&mut rig.state, &mut rig.runtime, &rig.arena, &mut adapted);
    assert!(refused.is_err());
    assert_eq!(
        failures(&adapted),
        BTreeMap::from([("record:CopyTexture:CopyMismatch", 1)])
    );
    assert_eq!(adapted.report.phases.refusals, 1);

    camera_scene(
        &mut world,
        "default_white",
        Some(TextureRef::Render(written)),
    );
    let (adapted, _operations, _) = rig.frame(&mut world, &[]);
    assert_eq!(
        adapted.report.stand_ins.by_property.get(UNWRITTEN),
        Some(&1),
        "{:?}",
        adapted.report.stand_ins
    );
}

/// A bake as the game runs one: a camera draws into staging, its levels are generated, it is
/// copied into a destination whose flag asks for levels, and a blit samples the destination. With
/// `refused`, a copy that cannot be made sits between the bake's records. Returns the frame's
/// report and the destination's level zero and level one.
fn bake(refused: bool) -> (Adapted, Vec<u8>, Vec<u8>) {
    let mut rig = Rig::new();
    let mut world = world();
    let staging = mipped(&mut world, 3, false);
    let destination = mipped(&mut world, 3, true);
    let out = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, staging);
    world
        .record_offscreen(OffscreenWork::GenerateMips { texture: staging })
        .unwrap();
    if refused {
        let odd = render_texture(&mut world, [2 * SIDE, 2 * SIDE]);
        copy(&mut world, staging, odd);
    }
    copy(&mut world, staging, destination);
    let blit = shader(&mut world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(destination),
            target: TargetRef::Render(out),
            material: Some(material),
            pass: -1,
        })
        .unwrap();
    let (adapted, mut operations, _) = rig.frame(&mut world, &[]);
    let name = ids::render_texture(destination);
    let levels = [0, 1].map(|mip| operations.read_back(name, mip, 0).unwrap());
    rig.run(operations);
    let [base, level] = levels.map(|image| image.bytes(&rig.device));
    (adapted, base, level)
}

#[test]
fn a_refused_record_beside_a_bake_leaves_every_bake_record_submitted_and_the_destination_filled() {
    let (clean, clean_base, clean_level) = bake(false);
    let (with_refusal, base, level) = bake(true);
    assert_eq!(failures(&clean), BTreeMap::new());
    assert_eq!(
        failures(&with_refusal),
        BTreeMap::from([("record:CopyTexture:CopyMismatch", 1)])
    );
    assert_eq!(with_refusal.report.built, clean.report.built);
    assert_eq!(with_refusal.frame.records.len(), clean.frame.records.len());
    assert_camera_image(&base);
    assert_eq!((base, level), (clean_base, clean_level));
}
