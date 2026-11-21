//! A baked page copied into a texture that generates its own mips: the shape of a terrain page
//! bake. A staging render texture has a chain and no automatic generation. Its level 0 is written,
//! `GenerateMips` fills the levels below, and a copy of every level into a render texture whose
//! flag asks for automatic generation follows, as the managed `Graphics.CopyTexture(src, dst)`
//! records it. A draw that reads the destination at a minified scale with a trilinear sampler must
//! find the staging colour at every level, in the flush of the copy and in a later one.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture,
    WorldSampler, dimension,
};

use super::super::ids;
use super::gpu::{Rig, SIDE, camera_render, shader, vector};
use super::support::{render_texture, world};

const RGBA8: u32 = 8;
const MIPS: u32 = 3;
const TRILINEAR: u32 = 2;
const STAGING_COLOR: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
/// `STAGING_COLOR` as RGBA8 texels.
const STAGING_TEXEL: [u8; 4] = [64, 128, 191, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// A `SIDE` by `SIDE` render texture of `MIPS` levels with a trilinear sampler.
fn page(world: &mut World, auto: bool) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format: RGBA8,
            depth_bits: 0,
            mip_count: MIPS,
            srgb: false,
            dimension: dimension::TEX_2D,
            volume_depth: 1,
            auto_generate_mips: auto,
            sampler: WorldSampler {
                filter: TRILINEAR,
                ..WorldSampler::default()
            },
            generation: 0,
        })
        .unwrap()
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

/// A clear of level 0 of `target` to `STAGING_COLOR`.
fn clear_staging(world: &mut World, target: RenderTextureId) {
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
            value: STAGING_COLOR,
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
}

/// `RenderTexture.GenerateMips` on the staging texture, then `Graphics.CopyTexture(staging,
/// page)`: one copy per level both have, on element 0.
fn generate_and_copy(world: &mut World, staging: RenderTextureId, page: RenderTextureId) {
    record(world, OffscreenWork::GenerateMips { texture: staging });
    for mip in 0..MIPS {
        record(
            world,
            OffscreenWork::CopyTexture {
                source: TextureRef::Render(staging),
                source_element: 0,
                source_mip: mip,
                target: TargetRef::Render(page),
                target_element: 0,
                target_mip: mip,
            },
        );
    }
}

/// A material of the synthetic blit shader, whose pass 0 is the source times a white tint.
fn sampling_material(world: &mut World) -> MaterialId {
    let blit = shader(world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    material
}

/// Blits `page` into a new render texture of `side` by `side` texels, which minifies it by
/// `SIDE / side` and so selects level `log2(SIDE / side)`.
fn sample_into(
    world: &mut World,
    material: MaterialId,
    page: RenderTextureId,
    side: u32,
) -> RenderTextureId {
    let target = render_texture(world, [side, side]);
    record(
        world,
        OffscreenWork::Blit {
            source: TextureRef::Render(page),
            target: TargetRef::Render(target),
            material: Some(material),
            pass: 0,
        },
    );
    target
}

/// What one flush reports: the failures the frame counted, level 0 of each target read, and every
/// level of the page.
struct Flushed {
    failures: Vec<(String, usize)>,
    sampled: Vec<Vec<u8>>,
    levels: Vec<Vec<u8>>,
}

/// One flush: the frame's records built and submitted, with level 0 of each of `targets` and
/// every level of `page` read back afterwards.
fn flush(
    rig: &mut Rig,
    world: &mut World,
    page: RenderTextureId,
    targets: &[RenderTextureId],
) -> Flushed {
    let (adapted, mut operations, _) = rig.frame(world, &[]);
    let sampled = targets
        .iter()
        .map(|target| {
            operations
                .read_back(ids::render_texture(*target), 0, 0)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let levels = (0..MIPS)
        .map(|mip| {
            operations
                .read_back(ids::render_texture(page), mip, 0)
                .unwrap()
        })
        .collect::<Vec<_>>();
    rig.run(operations);
    let failures = adapted
        .report
        .failures
        .iter()
        .map(|(name, count)| (name.clone(), *count))
        .collect();
    let bytes = |images: Vec<_>| -> Vec<Vec<u8>> {
        images
            .into_iter()
            .map(
                |image: worldrunner_render_program::unity::offscreen::PendingImage| {
                    image.bytes(&rig.device)
                },
            )
            .collect()
    };
    Flushed {
        failures,
        sampled: bytes(sampled),
        levels: bytes(levels),
    }
}

/// Every texel of `bytes` is `expected`, within one step of rounding in each channel.
fn assert_uniform(bytes: &[u8], expected: [u8; 4], what: &str) {
    assert!(!bytes.is_empty(), "{what}: no texels");
    for (index, texel) in bytes.chunks_exact(4).enumerate() {
        for channel in 0..4 {
            assert!(
                texel[channel].abs_diff(expected[channel]) <= 1,
                "{what}: texel {index} is {texel:?}, expected {expected:?}"
            );
        }
    }
}

fn assert_no_partial_read(failures: &[(String, usize)]) {
    assert!(
        failures
            .iter()
            .all(|(name, _)| name != "draw:SampledWrittenLevelsOnly"),
        "{failures:?}"
    );
}

/// The destination of the copy and the targets that read it at levels 0, 1 and 2.
struct Scene {
    page: RenderTextureId,
    reads: [RenderTextureId; 3],
}

/// A world that bakes a uniform staging texture, copies it into the page, and samples the page
/// at three scales. `split` puts the sampling in the flush after the copy's.
fn scene(rig: &mut Rig, world: &mut World, split: bool) -> (Scene, Vec<(String, usize)>) {
    let staging = page(world, false);
    let destination = page(world, true);
    clear_staging(world, staging);
    generate_and_copy(world, staging, destination);
    let mut failures = Vec::new();
    if split {
        failures.extend(flush(rig, world, destination, &[]).failures);
    }
    let material = sampling_material(world);
    let reads =
        [SIDE, SIDE / 2, SIDE / 4].map(|side| sample_into(world, material, destination, side));
    let scene = Scene {
        page: destination,
        reads,
    };
    (scene, failures)
}

fn assert_page_reads_as_staging(split: bool) {
    let mut rig = Rig::new();
    let mut world = world();
    let (scene, mut failures) = scene(&mut rig, &mut world, split);
    let Flushed {
        failures: later,
        sampled,
        levels,
    } = flush(&mut rig, &mut world, scene.page, &scene.reads);
    failures.extend(later);
    assert_no_partial_read(&failures);
    assert!(failures.is_empty(), "{failures:?}");
    for (mip, level) in levels.iter().enumerate() {
        assert_uniform(level, STAGING_TEXEL, &format!("page level {mip}"));
    }
    for (read, bytes) in sampled.iter().enumerate() {
        assert_uniform(
            bytes,
            STAGING_TEXEL,
            &format!("page sampled at level {read}"),
        );
    }
}

#[test]
fn a_page_copied_into_a_texture_that_generates_its_mips_is_read_whole_in_a_later_flush() {
    assert_page_reads_as_staging(true);
}

#[test]
fn a_page_copied_into_a_texture_that_generates_its_mips_is_read_whole_in_the_flush_of_the_copy() {
    assert_page_reads_as_staging(false);
}

#[test]
fn every_level_of_a_camera_written_page_keeps_its_content_through_the_copy() {
    let mut rig = Rig::new();
    let mut world = world();
    let staging = page(&mut world, false);
    let destination = page(&mut world, true);
    camera_render(&mut world, staging);
    generate_and_copy(&mut world, staging, destination);
    flush(&mut rig, &mut world, destination, &[]);
    let material = sampling_material(&mut world);
    let reads = [SIDE, SIDE / 2].map(|side| sample_into(&mut world, material, destination, side));
    let Flushed {
        failures,
        sampled,
        levels,
    } = flush(&mut rig, &mut world, destination, &reads);
    assert_no_partial_read(&failures);
    // The camera drew red over the left half and the clear is blue: level 0 as four columns, and
    // level 1 as two, the first red and the second blue.
    for (row, texels) in levels[0].chunks_exact(16).enumerate() {
        let columns: Vec<&[u8]> = texels.chunks_exact(4).collect();
        assert_eq!(columns, [&RED[..], &RED, &BLUE, &BLUE], "level 0 row {row}");
    }
    for (row, texels) in levels[1].chunks_exact(8).enumerate() {
        let columns: Vec<&[u8]> = texels.chunks_exact(4).collect();
        assert_eq!(columns, [&RED[..], &BLUE], "level 1 row {row}");
    }
    assert_eq!(sampled[0], levels[0]);
    assert_eq!(sampled[1], levels[1]);
}
