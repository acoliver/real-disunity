//! A bake whose records cross the boundary between two parts of a flush. A camera draws into a
//! render texture, a blit copies it into a staging texture that has a mip chain, `GenerateMips`
//! fills the levels below, and one copy per level fills a destination texture. A draw that
//! samples the destination at a minified scale with a trilinear sampler reads one level of it,
//! so the texels of the sample at each scale are the destination's texels at that level.
//!
//! With a budget of one planned draw the camera and the records after it are built in different
//! parts. They must leave the destination as a flush that holds them all in one build does.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture,
    WorldSampler, dimension,
};

use super::super::report::Report;
use super::flush::{camera_render, host};
use super::gpu::{Quad, SIDE, shader, vector};
use super::support::{render_texture, snapshot, world};

const RGBA8: u32 = 8;
const MIPS: u32 = 3;
const TRILINEAR: u32 = 2;

/// A `SIDE` by `SIDE` render texture of `MIPS` levels with a trilinear sampler.
fn chained(world: &mut World, auto: bool) -> RenderTextureId {
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

fn blit(world: &mut World, material: MaterialId, source: RenderTextureId, to: RenderTextureId) {
    record(
        world,
        OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(to),
            material: Some(material),
            pass: 0,
        },
    );
}

/// The records of the bake: the camera's texture is blitted into `staging`, whose levels are
/// generated and copied one by one into `destination`, and `destination` is sampled at each
/// level into a texture of that level's size. The requests that read those samples come back.
fn bake(
    world: &mut World,
    drawn: RenderTextureId,
    chain: (RenderTextureId, RenderTextureId),
) -> Vec<u64> {
    let (staging, destination) = chain;
    let blit_shader = shader(world, "blit");
    let material = world.create_material(blit_shader).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
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
            value: [0.0; 4],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
    camera_render(world, drawn);
    blit(world, material, drawn, staging);
    record(world, OffscreenWork::GenerateMips { texture: staging });
    for mip in 0..MIPS {
        record(
            world,
            OffscreenWork::CopyTexture {
                source: TextureRef::Render(staging),
                source_element: 0,
                source_mip: mip,
                target: TargetRef::Render(destination),
                target_element: 0,
                target_mip: mip,
            },
        );
    }
    (0..MIPS)
        .map(|mip| {
            let side = SIDE >> mip;
            let read = render_texture(world, [side, side]);
            blit(world, material, destination, read);
            world
                .request_texture_pixels(read, 0, 0, side, side)
                .unwrap()
        })
        .collect()
}

/// What a flush of the bake made: the texels of the sample at each level, the builds it took,
/// and what a record failed or was left out for.
struct Baked {
    levels: Vec<Vec<f32>>,
    builds: usize,
    problems: Vec<String>,
}

/// Each way a flush can fail a record, leave one out or read a texture short, that the report
/// counted.
fn problems(report: &Report) -> Vec<String> {
    let counted = [
        (
            "failures",
            report.failures.is_empty(),
            format!("{:?}", report.failures),
        ),
        (
            "skipped",
            report.skipped.is_empty(),
            format!("{:?}", report.skipped),
        ),
        (
            "renderers skipped",
            report.renderers_skipped.is_empty(),
            format!("{:?}", report.renderers_skipped),
        ),
        (
            "refused reads",
            report.pixel_reads_refused.is_empty(),
            format!("{:?}", report.pixel_reads_refused),
        ),
        (
            "partial reads",
            report.partial_reads.is_empty(),
            format!("{:?}", report.partial_reads),
        ),
    ];
    counted
        .into_iter()
        .filter(|(_, none, _)| !none)
        .map(|(kind, _, found)| format!("{kind}: {found}"))
        .collect()
}

/// The bake flushed by a host with room for `budget` planned draws in one build.
fn flushed(budget: usize, auto: bool) -> Baked {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    host.set_draw_budget(budget);
    let mut world = world();
    let drawn = render_texture(&mut world, [SIDE, SIDE]);
    let staging = chained(&mut world, false);
    let destination = chained(&mut world, auto);
    let requests = bake(&mut world, drawn, (staging, destination));
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    let problems = problems(&outcome.report);
    let levels = requests
        .into_iter()
        .map(|request| world.take_texture_pixels(request).unwrap())
        .collect();
    Baked {
        levels,
        builds: outcome.report.phases.builds,
        problems,
    }
}

#[test]
fn a_bake_split_across_parts_leaves_every_mip_of_its_destination_as_one_build_does() {
    for auto in [false, true] {
        let whole = flushed(usize::MAX, auto);
        let parts = flushed(1, auto);
        assert!(
            whole.problems.is_empty(),
            "auto {auto}: {:?}",
            whole.problems
        );
        assert!(
            parts.problems.is_empty(),
            "auto {auto}: {:?}",
            parts.problems
        );
        // Level 2 is one texel, the mean of the camera's drawn half and its clear: the blue
        // channel is 0 where it drew and 1 where it did not.
        let blue = whole.levels[2][2];
        assert!(
            (blue - 0.5).abs() < 0.05,
            "auto {auto}: level 2 is {:?}",
            whole.levels[2]
        );
        assert!(
            parts.builds > whole.builds,
            "auto {auto}: {} builds in parts, {} whole",
            parts.builds,
            whole.builds
        );
        assert_eq!(parts.levels, whole.levels, "auto {auto}");
    }
}
