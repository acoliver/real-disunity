//! What a terrain frame costs: the commands it records, and the objects the program runtime
//! builds for it.
//!
//! Issue 450 requires that frame cost does not rise. The fixed path records `2 + 4C` commands
//! for C resident chunks (`3 + 4C` under a shadow map): one pipeline, the material group, the
//! shadow group when there is one, and per chunk two vertex buffers, one index buffer and one
//! draw. The runtime path is held to the same arithmetic and to a steady state that builds no
//! pipeline, module, bind group or set.

use super::capture_tests::{Rig, Source, Target, render, scenes, streaming::settle};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::renderer::legacy_terrain::Counted;

#[test]
fn the_fixed_terrain_path_records_two_plus_four_per_chunk_commands() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let frame = scenes::frame(scenes::pose());
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::streamed(9),
        (Source::Plain, frame),
    );
    settle(&context, &rig.gpu, &frame, 6);
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let mut chunks = 0;
    render(
        &context,
        target,
        |_| {},
        |pass| {
            chunks = rig
                .gpu
                .draw_color(pass, &built.pipelines, None)
                .terrain
                .calls;
        },
    );
    assert!(chunks > 1, "{chunks} chunks");
    let selected = rig.selected(&built.pipelines);
    let legacy = super::capture_tests::Legacy::new(&context, &rig.gpu, &selected);
    let mut counted = Counted::default();
    let drawn = legacy.draw(&mut counted, &rig.gpu, None);
    assert_eq!(drawn.calls, chunks);
    let chunks = u32::try_from(chunks).unwrap();
    assert_eq!(counted.total(), 2 + 4 * chunks);
    assert_eq!(
        (counted.pipelines, counted.groups),
        (1, 1),
        "one pipeline and the material group, once per pass"
    );
    assert_eq!(
        (counted.vertex_buffers, counted.index_buffers, counted.draws),
        (2 * chunks, chunks, chunks)
    );
}

/// Frames after the first build nothing on the program runtime: the first frame prepares the
/// terrain's pipeline and sets, and a steady state adds no pipeline, shader module, bind group,
/// prepared set or draw.
///
/// Red until terrain draws through the runtime, which is the next commit of the migration.
#[test]
#[ignore = "red until terrain draws through the program runtime (slice 1, commit 2)"]
fn steady_state_terrain_frames_build_no_pipelines_bind_groups_or_buffers() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let frame = scenes::frame(scenes::pose());
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let draw = || {
        render(
            &context,
            target,
            |_| {},
            |pass| {
                rig.gpu.draw_color(pass, &built.pipelines, None);
            },
        );
    };
    let before = built.pipelines.terrain_runtime_stats();
    draw();
    let first = built.pipelines.terrain_runtime_stats();
    assert!(
        first.pipelines_built > before.pipelines_built,
        "the first terrain frame builds its pipeline on the program runtime: {first:?}"
    );
    for _ in 0..4 {
        draw();
    }
    assert_eq!(
        built.pipelines.terrain_runtime_stats(),
        first,
        "steady-state frames build nothing"
    );
}
