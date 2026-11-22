//! What an object frame costs: the commands it records.
//!
//! Issue 450 requires that frame cost does not rise. A frame records the frame group once and
//! then, for each selected range (one batch of instances at one level of detail): the instance
//! buffer, the previous-instance buffer when the pipelines write velocity, one draw per material
//! slot, and a pipeline, a material group, a vertex buffer or an index buffer only when it differs
//! from the one bound before. Mesh levels share the chunks of buffers an upload packs them into, so
//! consecutive ranges bind the vertex and index buffers once until the chunk changes. A steady-state
//! frame builds nothing on the program runtime.

use std::ops::Range;

use super::capture_tests::{Rig, Target, render, scenes};
use crate::dynamic::capture_tests::{CONFIGS, build, context};

use scenes::Spec;
use worldrunner_render_plan::RenderScene;
use worldrunner_render_program::runtime::PassCommands;

/// How many commands of each kind a recording issues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counted {
    pub pipelines: u32,
    pub groups: u32,
    pub vertex_buffers: u32,
    pub index_buffers: u32,
    pub draws: u32,
}

impl Counted {
    /// Every command, of every kind.
    pub(crate) fn total(&self) -> u32 {
        self.pipelines + self.groups + self.vertex_buffers + self.index_buffers + self.draws
    }
}

/// How many of a pass's draws could be one multi-draw: the draws that follow one another with
/// no command between them, since a recording leaves out the commands a draw does not need.
///
/// Both counts are of runs, so a pass that draws `d` times in `r` runs would record `r` multi-draws
/// where it now records `d` draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Runs {
    /// Runs of draws that share the pipeline, every bind group and every vertex and index buffer.
    pub by_buffers: u32,
    /// Runs of draws that share the pipeline, every bind group and the instance buffers, however
    /// many vertex and index buffers they set, as when meshes share their buffers.
    pub by_state: u32,
    state_changed: bool,
    geometry_changed: bool,
}

impl Runs {
    fn state(&mut self) {
        self.state_changed = true;
        self.geometry_changed = true;
    }

    fn geometry(&mut self, slot: u32) {
        self.geometry_changed = true;
        // The instance and skin slots are state a run cannot span.
        if slot != 0 {
            self.state_changed = true;
        }
    }

    fn draw(&mut self, first: bool) {
        self.by_buffers += u32::from(first || self.geometry_changed);
        self.by_state += u32::from(first || self.state_changed);
        self.state_changed = false;
        self.geometry_changed = false;
    }
}

/// Counts the commands a recording issues into a [`PassCommands`], by kind, and its runs of draws.
#[derive(Default)]
pub(crate) struct Tally(pub Counted, pub Runs);

impl PassCommands for Tally {
    fn set_pipeline(&mut self, _: &wgpu::RenderPipeline) {
        self.0.pipelines += 1;
        self.1.state();
    }
    fn set_bind_group(&mut self, _: u32, _: &wgpu::BindGroup) {
        self.0.groups += 1;
        self.1.state();
    }
    fn set_vertex_buffer(&mut self, slot: u32, _: &wgpu::Buffer) {
        self.0.vertex_buffers += 1;
        self.1.geometry(slot);
    }
    fn set_index_buffer(&mut self, _: &wgpu::Buffer, _: wgpu::IndexFormat) {
        self.0.index_buffers += 1;
        self.1.geometry(0);
    }
    fn draw(&mut self, _: Range<u32>, _: Range<u32>) {
        self.1.draw(self.0.draws == 0);
        self.0.draws += 1;
    }
    fn draw_indexed(&mut self, _: Range<u32>, _: i32, _: Range<u32>) {
        self.1.draw(self.0.draws == 0);
        self.0.draws += 1;
    }
}

/// The commands the runtime path records for the same scene, after preparing it as a frame does,
/// with the runs of draws the recording holds. Preparing before the pass builds the pipelines, so
/// recording builds nothing.
fn through_runtime(index: usize, scene: &RenderScene, cards: usize) -> (Counted, Runs) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[index]);
    let mut rig = Rig::new(&context, &built.pipelines, scene);
    // The frame flushes the scene's instance updates in its own encoder and records its draws in
    // the same transaction, as the frame renderer does: a scene that tracks previous transforms
    // has updates pending to any other recording.
    let mut encoder = context.submission().begin(context.device()).unwrap();
    rig.gpu
        .flush_instances_encoded(context.device(), &mut encoder)
        .unwrap();
    rig.gpu
        .prepare_objects(&built.pipelines, &rig.frame)
        .unwrap();
    let prepared = built.pipelines.object_runtime_stats();
    let selected = rig.selected(&built.pipelines);
    let frame = rig.gpu.drawing_frame_group_for_test();
    let mut tally = Tally::default();
    drop(
        rig.gpu
            .draw_objects_prepared(&mut tally, &frame, selected.object_context()),
    );
    encoder.finish().submit().unwrap();
    let (counted, runs) = (tally.0, tally.1);
    assert_eq!(
        built.pipelines.object_runtime_stats(),
        prepared,
        "recording a prepared frame builds nothing"
    );
    println!(
        "OBJECT-COST-RUNTIME config={} cards={} pipelines={} groups={} vertex_buffers={} index_buffers={} draws={} total={} stats={prepared:?}",
        CONFIGS[index].name,
        cards,
        counted.pipelines,
        counted.groups,
        counted.vertex_buffers,
        counted.index_buffers,
        counted.draws,
        counted.total(),
    );
    println!(
        "OBJECT-COST-RUNS config={} cards={cards} draws={} runs_by_buffers={} runs_by_state={}",
        CONFIGS[index].name, counted.draws, runs.by_buffers, runs.by_state,
    );
    (counted, runs)
}

/// Eight cards with eight different keys, one batch each: eight pipelines, eight material
/// groups and the frame group, and per batch its instance buffer and one draw. The cards use two
/// meshes (plain and impostor) that share one chunk, so their vertex and index buffers bind once.
#[test]
fn the_runtime_path_records_one_batch_of_commands_per_card_with_its_own_key() {
    let scene = scenes::scene(&scenes::base_specs());
    let counted = through_runtime(2, &scene, 8).0;
    assert_eq!(
        counted,
        Counted {
            pipelines: 8,
            groups: 1 + 8,
            vertex_buffers: 8 + 1,
            index_buffers: 1,
            draws: 8,
        }
    );
    assert_eq!(counted.total(), 35);
}

/// A velocity pipeline adds the previous-instance buffer to every batch and nothing else.
#[test]
fn the_runtime_path_adds_the_previous_instance_buffer_under_velocity() {
    let scene = scenes::scene(&scenes::base_specs());
    let plain = through_runtime(2, &scene, 8).0;
    let recorded = through_runtime(4, &scene, 8).0;
    assert_eq!(
        recorded,
        Counted {
            vertex_buffers: plain.vertex_buffers + 8,
            ..plain
        }
    );
    assert_eq!(recorded.total(), 43);
}

/// Cards with one key and a material each share the pipeline bind and rebind their material
/// group: one pipeline, a group for the frame and one per material, and per batch its instance
/// buffer and draw. The six share one mesh, so its vertex and index buffers bind once.
#[test]
fn the_runtime_path_sets_a_pipeline_only_when_the_key_changes() {
    let scene = scenes::scene(&[Spec::default(); 6]);
    let counted = through_runtime(2, &scene, 6).0;
    assert_eq!(
        counted,
        Counted {
            pipelines: 1,
            groups: 1 + 6,
            vertex_buffers: 6 + 1,
            index_buffers: 1,
            draws: 6,
        },
        "six materials share one pipeline and one mesh: one set_pipeline, one vertex and index \
         buffer, a group for the frame and one each"
    );
}

/// Cards that share a material are one batch and one instanced draw, however many there are.
#[test]
fn the_runtime_path_draws_cards_that_share_a_material_with_one_instanced_draw() {
    let scene = scenes::shared(Spec::default(), 6);
    let counted = through_runtime(2, &scene, 6).0;
    assert_eq!(
        counted,
        Counted {
            pipelines: 1,
            groups: 2,
            vertex_buffers: 2,
            index_buffers: 1,
            draws: 1,
        }
    );
}

/// The frames after the first build nothing on the program runtime: the upload prepares the
/// materials' sets, the first frame prepares the pipelines of the pairs it draws, and a steady
/// state adds no pipeline, shader module, bind group or set.
#[test]
fn steady_state_object_frames_build_no_pipelines_bind_groups_or_sets() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let mut rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::scene(&scenes::base_specs()),
    );
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let draw = |rig: &mut Rig<'_>| {
        render(&context, target, &mut rig.gpu, |pass, gpu| {
            gpu.draw_color(pass, &built.pipelines, None);
        });
    };
    let uploaded = built.pipelines.object_runtime_stats();
    assert_eq!(
        uploaded.sets_prepared, 8,
        "one set for each of the eight materials"
    );
    draw(&mut rig);
    let first = built.pipelines.object_runtime_stats();
    assert!(
        first.pipelines_built > uploaded.pipelines_built,
        "the first object frame builds its pipelines on the program runtime: {first:?}"
    );
    assert_eq!(first.sets_prepared, uploaded.sets_prepared);
    for _ in 0..4 {
        draw(&mut rig);
    }
    assert_eq!(
        built.pipelines.object_runtime_stats(),
        first,
        "steady-state frames build nothing"
    );
    println!(
        "OBJECT-COST steady_state uploaded={uploaded:?} after_first={first:?} after_four_more=unchanged"
    );
}

/// A frame that prepares before its pass opens finds everything made, so drawing builds nothing.
#[test]
fn preparing_before_the_pass_builds_every_pipeline_the_pass_draws_with() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let mut rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::scene(&scenes::base_specs()),
    );
    let before = built.pipelines.object_runtime_stats();
    rig.gpu
        .prepare_objects(&built.pipelines, &rig.frame)
        .unwrap();
    let prepared = built.pipelines.object_runtime_stats();
    assert!(prepared.pipelines_built > before.pipelines_built);
    render(
        &context,
        Target {
            formats: &built.formats,
            samples: 1,
        },
        &mut rig.gpu,
        |pass, gpu| {
            gpu.draw_color(pass, &built.pipelines, None);
        },
    );
    assert_eq!(built.pipelines.object_runtime_stats(), prepared);
}

/// Two scenes drawn with one set of pipelines share the compiled pipelines.
#[test]
fn a_second_scene_shares_the_pipelines_the_first_compiled() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let specs = scenes::base_specs();
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let mut stats = Vec::new();
    for _ in 0..2 {
        let mut rig = Rig::new(&context, &built.pipelines, &scenes::scene(&specs));
        render(&context, target, &mut rig.gpu, |pass, gpu| {
            gpu.draw_color(pass, &built.pipelines, None);
        });
        stats.push(built.pipelines.object_runtime_stats());
    }
    assert_eq!(
        stats[1].pipelines_built, stats[0].pipelines_built,
        "the second scene compiles no pipeline"
    );
    assert_eq!(stats[1].modules_built, stats[0].modules_built);
    assert_eq!(
        stats[1].sets_prepared,
        stats[0].sets_prepared + 8,
        "it prepares the sets of its own materials"
    );
}

/// The metadata prepass prepares its object pipelines before its pass opens, as the scene
/// feature and the shadowed frame do, so the pass finds them built; steady-state prepass frames
/// build nothing on the program runtime.
#[test]
fn the_object_prepass_prepares_before_its_pass_and_steady_state_frames_build_nothing() {
    use super::capture_tests::{prepass, render_with_depth};
    use crate::renderer::depth::DepthMode;
    use crate::scene_metadata::MetadataOutput;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = scenes::frame(scenes::pose());
    for output in [MetadataOutput::Normal, MetadataOutput::Merged] {
        for mode in [DepthMode::Independent, DepthMode::Shared] {
            let (built, group, formats) = prepass::pipelines(&context, &frame, output);
            let prepass = built.for_depth(mode);
            let mut rig = Rig::moving(&context, &prepass, &scenes::scene(&prepass::specs()), false);
            let before = prepass.object_runtime_stats();
            rig.gpu.prepare_objects(&prepass, &rig.frame).unwrap();
            let prepared = prepass.object_runtime_stats();
            assert!(
                prepared.pipelines_built > before.pipelines_built,
                "{output:?} {mode:?}: preparing builds the prepass object pipelines: {prepared:?}"
            );
            let target = Target {
                formats: &formats,
                samples: 1,
            };
            for frame in 0..5 {
                if frame > 0 {
                    rig.gpu.prepare_objects(&prepass, &rig.frame).unwrap();
                }
                rig.prime();
                render_with_depth(
                    &context,
                    target,
                    mode.format(),
                    &mut rig.gpu,
                    |pass, gpu| {
                        drop(gpu.draw_prepass(pass, &prepass, &group));
                    },
                );
                assert_eq!(
                    prepass.object_runtime_stats(),
                    prepared,
                    "{output:?} {mode:?}: frame {frame} found everything prepared"
                );
            }
            println!(
                "OBJECT-COST prepass output={output:?} depth={mode:?} before={before:?} after_prepare={prepared:?} after_five_frames=unchanged"
            );
        }
    }
}

/// Twelve batches of one pipeline key that differ only in mesh (three) and material (four) are
/// recorded grouped: the pass binds each of the four materials once, and the three meshes share a
/// chunk, so the vertex and index buffers bind once.
#[test]
fn grouped_draws_bind_each_material_once() {
    let scene = scenes::grid(3, 4);
    let (grouped, _) = through_runtime(2, &scene, 12);
    assert_eq!(
        grouped,
        Counted {
            pipelines: 1,
            groups: 1 + 4,
            vertex_buffers: grouped.vertex_buffers,
            index_buffers: 1,
            draws: 12,
        },
        "each material binds once and the meshes' chunk binds once: {grouped:?}"
    );
}

/// How many multi-draws the grouped pass could record, with the walk's batches as they are: none
/// fewer than it draws. A batch owns its instance buffer, so no two consecutive draws of the
/// scene share every buffer even though the meshes share theirs, and a multi-draw of one run
/// would replace each draw. A moving-camera frame over a compacted view shares one instance
/// buffer between batches, where the meshes' shared buffers make a material's draws one run.
#[test]
fn every_grouped_draw_of_the_test_scenes_is_a_run_of_its_own() {
    let cases = [
        ("grid", scenes::grid(3, 4), 12),
        ("six materials", scenes::scene(&[Spec::default(); 6]), 6),
        ("base", scenes::scene(&scenes::base_specs()), 8),
    ];
    for (name, scene, draws) in cases {
        let (counted, runs) = through_runtime(2, &scene, draws);
        assert_eq!(counted.draws, u32::try_from(draws).unwrap(), "{name}");
        assert_eq!(
            (runs.by_buffers, runs.by_state),
            (counted.draws, counted.draws),
            "{name}: a run of two draws would need a shared buffer pair"
        );
    }
}

/// Cards of one material are one batch and one draw, which is one run.
#[test]
fn one_instanced_draw_is_one_run() {
    let (counted, runs) = through_runtime(2, &scenes::shared(Spec::default(), 6), 6);
    assert_eq!((counted.draws, runs.by_buffers, runs.by_state), (1, 1, 1));
}
