//! What an object frame costs on the fixed path: the commands it records.
//!
//! Issue 450 requires that frame cost does not rise. The fixed path records the frame group
//! once and then, for each selected range (one batch of instances at one level of detail): the
//! instance buffer, the previous-instance buffer when the pipelines write velocity, the vertex
//! buffer and the index buffer, one draw per material slot, and a pipeline or a material group
//! only when it differs from the one bound before. These numbers are the baseline the runtime
//! path is held to: it records the same commands, and a steady-state frame builds nothing on the
//! program runtime.

use std::ops::Range;

use super::capture_tests::{Legacy, Rig, Target, counted, render, scenes};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::renderer::legacy_objects::{Counted, Recipe, compose};

use scenes::Spec;
use worldrunner_render_plan::RenderScene;
use worldrunner_render_program::runtime::PassCommands;

/// Counts the commands a recording issues into a [`PassCommands`], by kind, as [`Counted`] does
/// for the frozen fixed path.
#[derive(Default)]
pub(crate) struct Tally(pub Counted);

impl PassCommands for Tally {
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

fn baseline(index: usize, scene: &RenderScene, cards: usize) -> (Counted, u64) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[index]);
    let rig = Rig::new(&context, &built.pipelines, scene);
    let recipe = match index {
        4 => Recipe {
            velocity: true,
            ..Recipe::default()
        },
        _ => Recipe::default(),
    };
    let legacy = Legacy::new(&context, &rig.selected(&built.pipelines), &compose(recipe));
    let (counted, recorded) = counted(&rig, &legacy);
    println!(
        "OBJECT-COST config={} cards={} pipelines={} groups={} vertex_buffers={} index_buffers={} draws={} total={} instances={}",
        CONFIGS[index].name,
        cards,
        counted.pipelines,
        counted.groups,
        counted.vertex_buffers,
        counted.index_buffers,
        counted.draws,
        counted.total(),
        recorded.instances
    );
    assert_eq!(u64::from(counted.draws), recorded.calls);
    (counted, recorded.instances)
}

/// Eight cards with eight different keys, one batch each: eight pipelines, eight material
/// groups and the frame group, and per batch two vertex buffers, one index buffer and one draw.
#[test]
fn the_fixed_object_path_records_one_batch_of_commands_per_card_with_its_own_key() {
    let (counted, instances) = baseline(2, &scenes::scene(&scenes::base_specs()), 8);
    assert_eq!(instances, 8);
    assert_eq!(
        counted,
        Counted {
            pipelines: 8,
            groups: 1 + 8,
            vertex_buffers: 2 * 8,
            index_buffers: 8,
            draws: 8,
        }
    );
    assert_eq!(counted.total(), 49);
}

/// A velocity pipeline adds the previous-instance buffer to every batch and nothing else.
#[test]
fn velocity_adds_one_vertex_buffer_per_batch_to_the_fixed_object_path() {
    let (plain, _) = baseline(2, &scenes::scene(&scenes::base_specs()), 8);
    let (velocity, _) = baseline(4, &scenes::scene(&scenes::base_specs()), 8);
    assert_eq!(
        velocity,
        Counted {
            vertex_buffers: plain.vertex_buffers + 8,
            ..plain
        }
    );
    assert_eq!(velocity.total(), 57);
}

/// Cards with one key and a material each share the pipeline bind and rebind their material
/// group: one pipeline, a group for the frame and one per material, and per batch the same
/// buffers and draw as before.
#[test]
fn the_fixed_object_path_sets_a_pipeline_only_when_the_key_changes() {
    let specs = vec![Spec::default(); 6];
    let (counted, instances) = baseline(2, &scenes::scene(&specs), 6);
    assert_eq!(instances, 6);
    assert_eq!(
        counted,
        Counted {
            pipelines: 1,
            groups: 1 + 6,
            vertex_buffers: 2 * 6,
            index_buffers: 6,
            draws: 6,
        }
    );
}

/// Cards that share a material are one batch and one instanced draw, however many there are.
#[test]
fn the_fixed_object_path_draws_cards_that_share_a_material_with_one_instanced_draw() {
    let (counted, instances) = baseline(2, &scenes::shared(Spec::default(), 6), 6);
    assert_eq!(instances, 6);
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

/// The commands the runtime path records for the same scene, after preparing it as a frame does.
/// Preparing before the pass builds the pipelines, so recording builds nothing.
fn through_runtime(index: usize, scene: &RenderScene, cards: usize) -> Counted {
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
    let frame = rig.gpu.legacy_objects().frame;
    let mut tally = Tally::default();
    drop(
        rig.gpu
            .draw_objects_prepared(&mut tally, &frame, selected.object_context()),
    );
    encoder.finish().submit().unwrap();
    let counted = tally.0;
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
    counted
}

#[test]
fn the_runtime_path_records_the_commands_of_the_fixed_path_for_cards_with_their_own_key() {
    let scene = scenes::scene(&scenes::base_specs());
    assert_eq!(through_runtime(2, &scene, 8), baseline(2, &scene, 8).0);
}

#[test]
fn the_runtime_path_adds_the_previous_instance_buffer_the_fixed_path_adds_under_velocity() {
    let scene = scenes::scene(&scenes::base_specs());
    let (fixed, _) = baseline(4, &scene, 8);
    let recorded = through_runtime(4, &scene, 8);
    assert_eq!(recorded, fixed);
    assert_eq!(recorded.total(), 57);
}

#[test]
fn the_runtime_path_sets_a_pipeline_only_when_the_key_changes() {
    let scene = scenes::scene(&[Spec::default(); 6]);
    let counted = through_runtime(2, &scene, 6);
    assert_eq!(counted, baseline(2, &scene, 6).0);
    assert_eq!(
        (counted.pipelines, counted.groups),
        (1, 1 + 6),
        "six materials share one pipeline: one set_pipeline, a group for the frame and one each"
    );
}

#[test]
fn the_runtime_path_draws_cards_that_share_a_material_with_one_instanced_draw() {
    let scene = scenes::shared(Spec::default(), 6);
    let counted = through_runtime(2, &scene, 6);
    assert_eq!(counted, baseline(2, &scene, 6).0);
    assert_eq!(counted.draws, 1);
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
