//! What an object frame costs on the fixed path: the commands it records.
//!
//! Issue 450 requires that frame cost does not rise. The fixed path records the frame group
//! once and then, for each selected range (one batch of instances at one level of detail): the
//! instance buffer, the previous-instance buffer when the pipelines write velocity, the vertex
//! buffer and the index buffer, one draw per material slot, and a pipeline or a material group
//! only when it differs from the one bound before. These numbers are the baseline the runtime
//! path is held to.

use super::capture_tests::{Legacy, Rig, counted, scenes};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::renderer::legacy_objects::{Counted, Recipe, compose};

use scenes::Spec;
use worldrunner_render_plan::RenderScene;

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
