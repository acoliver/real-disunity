//! A scene whose batches all have nothing to upload is not walked again until something changes.
//!
//! The scene is 48 cards in three batches (materials 0, 1 and 2). A step flushes the instances
//! the way a frame does and stages the history publication, so the scene settles through the
//! same publications a real frame makes.
use super::compaction::Rig;
use super::compaction_gather::{fragmented, object};
use crate::draw::InstanceWrite;
use worldrunner_render_gpu::submission::{PreparedFrame, TransactionError};
use worldrunner_render_post::PrepareContext;

/// How many batch loops a step ran and the writes it returned.
struct Step {
    loops: usize,
    writes: Vec<InstanceWrite>,
    frame: PreparedFrame,
}

fn step(rig: &mut Rig) -> Step {
    let Rig { context, gpu, .. } = rig;
    let before = gpu.instances.flush_loops();
    let mut writes = Vec::new();
    let frame = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let mut prepare = PrepareContext::new(context.device(), encoder)?;
            writes = gpu.prepare_instances(&mut prepare);
            gpu.publish_instance_history(&prepare);
            prepare.finish();
            Ok::<_, TransactionError>(())
        })
        .unwrap();
    Step {
        loops: gpu.instances.flush_loops() - before,
        writes,
        frame,
    }
}

fn submit(rig: &Rig, frame: PreparedFrame) {
    let submission = frame.submit().unwrap();
    rig.context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
}

/// Steps until a flush skips the batches, and returns the loops each step ran.
fn settle(rig: &mut Rig) -> Vec<usize> {
    let mut loops = Vec::new();
    for _ in 0..8 {
        let step = step(rig);
        loops.push(step.loops);
        submit(rig, step.frame);
        if step.loops == 0 {
            return loops;
        }
    }
    panic!("the scene never settled: {loops:?}");
}

fn settled_rig() -> Rig {
    let mut rig = Rig::over(&fragmented(), false);
    settle(&mut rig);
    rig
}

/// Steps that must neither loop nor write.
fn assert_at_rest(rig: &mut Rig) {
    for _ in 0..4 {
        let step = step(rig);
        assert_eq!(step.loops, 0, "a scene at rest was walked");
        assert!(step.writes.is_empty());
        submit(rig, step.frame);
    }
}

#[test]
fn a_scene_at_rest_is_walked_once_and_then_not_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&fragmented(), false);
    let loops = settle(&mut rig);
    assert!(loops.len() <= 4, "{loops:?}");
    assert_eq!(loops.last(), Some(&0));
    assert!(
        loops[..loops.len() - 1].iter().all(|&walked| walked == 1),
        "{loops:?}"
    );
    assert_at_rest(&mut rig);
}

#[test]
fn moving_frames_over_a_scene_at_rest_do_not_walk_its_batches() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    let walked = rig.gpu.instances.flush_loops();
    for x in [0, 700, 1_400, 2_100] {
        rig.render(&super::compaction::unlit(super::compaction::pose_at(x)));
    }
    assert_eq!(rig.gpu.instances.flush_loops(), walked);
}

#[test]
fn an_edit_is_written_once_and_the_scene_settles_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    let handle = rig.gpu.instance_handles().nth(7).unwrap();
    let mut transform = *object(7).transform();
    transform.position_millimeters[0] += 600;
    rig.gpu.set_instance_transform(handle, transform).unwrap();
    let edited = step(&mut rig);
    assert_eq!(edited.loops, 1);
    assert_eq!(
        edited.writes,
        [InstanceWrite {
            mesh: 0,
            material: 1,
            instances: 2..3
        }]
    );
    submit(&rig, edited.frame);
    settle(&mut rig);
    assert_at_rest(&mut rig);
}

#[test]
fn a_cancelled_transaction_leaves_its_edit_to_be_written_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    let handle = rig.gpu.instance_handles().nth(7).unwrap();
    let mut transform = *object(7).transform();
    transform.position_millimeters[1] += 600;
    rig.gpu.set_instance_transform(handle, transform).unwrap();
    let first = step(&mut rig);
    first.frame.cancel();
    let second = step(&mut rig);
    assert_eq!(second.loops, 1, "the cancelled edit was skipped");
    assert_eq!(second.writes, first.writes);
    assert!(!second.writes.is_empty());
    submit(&rig, second.frame);
    settle(&mut rig);
    assert_at_rest(&mut rig);
}

#[test]
fn a_history_publication_alone_ends_the_rest() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    rig.gpu.reset_velocity_history();
    assert!(
        rig.gpu
            .instances
            .batches
            .values()
            .any(|batch| batch.history.has_pending())
    );
    let reset = step(&mut rig);
    assert_eq!(reset.loops, 1, "the pending history was skipped");
    submit(&rig, reset.frame);
    assert!(
        rig.gpu
            .instances
            .batches
            .values()
            .all(|batch| !batch.history.has_pending())
    );
    settle(&mut rig);
    assert_at_rest(&mut rig);
}

#[test]
fn growing_the_batches_is_uploaded_and_the_scene_settles_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    for index in 0..40 {
        rig.gpu.add_instance(object(index), true).unwrap();
    }
    let grown = step(&mut rig);
    assert_eq!(grown.loops, 1);
    let written: u32 = grown
        .writes
        .iter()
        .map(|write| write.instances.end - write.instances.start)
        .sum();
    assert_eq!(written, 40);
    submit(&rig, grown.frame);
    settle(&mut rig);
    assert_at_rest(&mut rig);
}

#[test]
fn toggling_previous_transforms_restages_the_batches_each_way() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = settled_rig();
    rig.gpu.track_previous_transforms(true);
    let on = settle(&mut rig);
    assert!(on[0] == 1 && on.len() >= 2, "{on:?}");
    assert!(
        rig.gpu
            .instances
            .batches
            .values()
            .all(|batch| batch.previous.is_some())
    );
    assert_at_rest(&mut rig);
    rig.gpu.track_previous_transforms(false);
    let off = step(&mut rig);
    assert_eq!(off.loops, 1, "the toggle was not noticed");
    submit(&rig, off.frame);
    assert_at_rest(&mut rig);
}
