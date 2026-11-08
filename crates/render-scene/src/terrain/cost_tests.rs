//! What a terrain frame costs: the commands it records, and the objects the program runtime
//! builds for it.
//!
//! Issue 450 requires that frame cost does not rise. The fixed path records `2 + 4C` commands
//! for C resident chunks (`3 + 4C` under a shadow map): one pipeline, the material group, the
//! shadow group when there is one, and per chunk two vertex buffers, one index buffer and one
//! draw. The runtime path is held to the same arithmetic and to a steady state that builds no
//! pipeline, module, bind group or set.

use super::capture_tests::{
    Rig, Source, Target, prepass::pipelines as prepass_pipelines, render, render_depth, scenes,
    streaming::settle,
};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::renderer::legacy_terrain::Counted;
use std::ops::Range;
use worldrunner_render_program::runtime::PassCommands;

/// Counts the commands the runtime path issues, by kind, as [`Counted`] does for the fixed path.
#[derive(Default)]
struct Recorded(Counted);

impl PassCommands for Recorded {
    fn set_pipeline(&mut self, _: &wgpu::RenderPipeline) {
        self.0.pipelines += 1;
    }
    fn set_bind_group(&mut self, _: u32, _: &wgpu::BindGroup) {
        self.0.groups += 1;
    }
    fn set_vertex_buffer(&mut self, _: u32, _: &wgpu::Buffer) {
        self.0.vertex_buffers += 1;
    }
    fn set_index_buffer(&mut self, _: &wgpu::Buffer, _: wgpu::IndexFormat) {
        self.0.index_buffers += 1;
    }
    fn draw(&mut self, _: Range<u32>, _: Range<u32>) {
        self.0.draws += 1;
    }
    fn draw_indexed(&mut self, _: Range<u32>, _: i32, _: Range<u32>) {
        self.0.draws += 1;
    }
}

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
#[test]
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
    println!(
        "TERRAIN-COST steady_state before={before:?} after_first={first:?} after_four_more=unchanged"
    );
}

/// A streaming frame on the runtime records the commands the fixed path recorded, chunk for
/// chunk, and while the same chunks stay resident it builds nothing on the runtime.
#[test]
fn a_streaming_frame_records_the_same_draw_calls_as_the_fixed_path() {
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
    let selected = rig.selected(&built.pipelines);
    let legacy = super::capture_tests::Legacy::new(&context, &rig.gpu, &selected);
    let mut fixed = Counted::default();
    let drawn = legacy.draw(&mut fixed, &rig.gpu, None);
    let mut first = Recorded::default();
    let (runtime, bound) = rig.gpu.draw_terrain_runtime(&mut first, &selected, None);
    assert!(bound && runtime.calls == drawn.calls && runtime.calls > 1);
    assert_eq!(
        first.0, fixed,
        "the runtime records the fixed path's commands"
    );
    assert_eq!(first.0.total(), 2 + 4 * u32::try_from(drawn.calls).unwrap());
    let built_by_first_frame = built.pipelines.terrain_runtime_stats();
    assert!(built_by_first_frame.pipelines_built > 0);
    assert!(built_by_first_frame.sets_prepared > 0);
    for _ in 0..3 {
        let mut again = Recorded::default();
        rig.gpu.draw_terrain_runtime(&mut again, &selected, None);
        assert_eq!(again.0, fixed);
    }
    assert_eq!(
        built.pipelines.terrain_runtime_stats(),
        built_by_first_frame,
        "frames over the same chunks build nothing on the runtime"
    );
    println!(
        "TERRAIN-COST chunks={} commands={} fixed_commands={} first_frame={built_by_first_frame:?}",
        drawn.calls,
        first.0.total(),
        fixed.total()
    );
}

/// Terrain that draws through the runtime compiles none of the fixed pipelines, and a second
/// scene on the same pipelines finds the first scene's compiled pipeline.
#[test]
fn a_second_scene_shares_the_compiled_pipeline_and_the_fixed_pipelines_stay_unbuilt() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let frame = scenes::frame(scenes::pose());
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let draw = |rig: &Rig<'_>| {
        render(
            &context,
            target,
            |_| {},
            |pass| {
                rig.gpu.draw_color(pass, &built.pipelines, None);
            },
        );
    };
    let first = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    draw(&first);
    let after_first = built.pipelines.terrain_runtime_stats();
    let second = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    draw(&second);
    let after_second = built.pipelines.terrain_runtime_stats();
    assert_eq!(after_second.pipelines_built, after_first.pipelines_built);
    assert_eq!(after_second.modules_built, after_first.modules_built);
    assert_eq!(
        after_second.sets_prepared,
        after_first.sets_prepared + 1,
        "each scene prepares its own material set"
    );
    assert!(!built.pipelines.fixed_terrain_built());
}

/// The metadata prepass prepares its pipeline before the pass opens, so the pass builds nothing,
/// and frames after the first build nothing either, for both outputs and all depth modes the
/// prepass is drawn in.
#[test]
fn the_prepass_prepares_before_its_pass_and_steady_state_frames_build_nothing() {
    use crate::renderer::depth::DepthMode;
    use crate::scene_metadata::MetadataOutput;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = scenes::frame(scenes::pose());
    for (name, output) in [
        ("normal", MetadataOutput::Normal),
        ("merged", MetadataOutput::Merged),
    ] {
        for mode in [DepthMode::Independent, DepthMode::Shared] {
            let (built, group, formats) = prepass_pipelines(&context, &frame, output);
            let rig = Rig::new(&context, &built, &scenes::plain(), (Source::Plain, frame));
            let prepass = built.for_depth(mode);
            let before = prepass.terrain_runtime_stats();
            rig.gpu.prepare_terrain(&prepass, &frame).unwrap();
            let prepared = prepass.terrain_runtime_stats();
            assert!(
                prepared.pipelines_built > before.pipelines_built,
                "{name}: preparing builds the prepass pipeline: {prepared:?}"
            );
            let target = Target {
                formats: &formats,
                samples: 1,
            };
            let draw = || {
                render_depth(
                    &context,
                    target,
                    (mode, None),
                    |_| {},
                    |pass| {
                        rig.gpu.draw_prepass(pass, &prepass, &group);
                    },
                );
            };
            draw();
            assert_eq!(
                prepass.terrain_runtime_stats(),
                prepared,
                "{name} {mode:?}: the pass found everything prepared"
            );
            for _ in 0..4 {
                rig.gpu.prepare_terrain(&prepass, &frame).unwrap();
                draw();
            }
            assert_eq!(
                prepass.terrain_runtime_stats(),
                prepared,
                "{name} {mode:?}: steady-state prepass frames build nothing"
            );
            println!(
                "TERRAIN-COST prepass output={name} depth={mode:?} before={before:?} after_prepare={prepared:?} after_five_frames=unchanged"
            );
        }
    }
}

/// A term replay prepares before its pass opens, and a capture that replays again with the same
/// pipelines builds nothing. Pipelines made for one replay and dropped do not accumulate in the
/// terrain's caches.
#[test]
fn term_replays_prepare_before_the_pass_build_nothing_in_steady_state_and_leave_no_residue() {
    use crate::compositor::terms::{CaptureTerm, TermCapture, pipelines_for};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let frame = scenes::frame(scenes::pose());
    let seed = pipelines_for(device, CaptureTerm::Albedo, None, None);
    let rig = Rig::new(&context, &seed, &scenes::plain(), (Source::Plain, frame));
    let replay = |capture: &TermCapture, pipelines: &crate::renderer::ScenePipelines| {
        let mut encoder = context.submission().begin(device).unwrap();
        capture.replay(&mut encoder, &rig.gpu, pipelines, (None, None));
        encoder.finish().submit().unwrap();
    };
    let capture = TermCapture::new(
        device,
        [crate::dynamic::capture_tests::SIZE; 2],
        CaptureTerm::Albedo,
    );
    let pipelines = pipelines_for(device, CaptureTerm::Albedo, None, None);
    let before = pipelines.terrain_runtime_stats();
    replay(&capture, &pipelines);
    let first = pipelines.terrain_runtime_stats();
    assert!(first.pipelines_built > before.pipelines_built, "{first:?}");
    for _ in 0..4 {
        replay(&capture, &pipelines);
    }
    assert_eq!(pipelines.terrain_runtime_stats(), first);
    drop(pipelines);
    for term in [
        CaptureTerm::DirectDiffuse,
        CaptureTerm::SunSpecular,
        CaptureTerm::Albedo,
        CaptureTerm::AmbientDiffuse,
        CaptureTerm::SunVisibility,
    ] {
        let fresh = pipelines_for(device, term, None, None);
        replay(&capture, &fresh);
    }
    let (pipelines_kept, sets_kept) = rig.gpu.kept_terrain();
    assert!(
        pipelines_kept <= 2 && sets_kept <= 2,
        "replays of dropped pipelines left {pipelines_kept} pipelines and {sets_kept} sets"
    );
    println!(
        "TERRAIN-COST term_replay first={first:?} after_four_more=unchanged kept_after_five_fresh_replays=({pipelines_kept}, {sets_kept})"
    );
}
