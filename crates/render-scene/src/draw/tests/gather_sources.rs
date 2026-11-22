//! A view gathers only batches whose buffers are flushed, and answers that for a scene whose
//! batches are all flushed without asking each selected batch.
//!
//! The scene is the 48 cards of the gather tests with a second, unused mesh, so a placement
//! added later can start a fourth batch that no frame has flushed.
use super::compaction::{Rig, mesh, pose_at, unlit};
use super::compaction_gather::{fragmented, object};
use super::*;
use worldrunner_render_gpu::submission::TransactionError;
use worldrunner_render_post::PrepareContext;

fn two_meshes() -> RenderScene {
    let source = fragmented();
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(vec![mesh(), mesh()], source.geometry().objects().to_vec()).unwrap(),
            None,
        )
        .unwrap()
}

/// A placement of the second mesh, which no batch holds until it is added.
fn of_second_mesh(index: i64) -> SceneObject {
    let first = object(index);
    SceneObject::new(
        *first.transform(),
        MeshHandle::from_raw(1),
        first.material(),
    )
}

/// Prepares the camera view's gather without flushing any batch first, and commits it unless
/// `cancel`. Returns how many ranges the gather staged.
fn gather_unflushed(rig: &mut Rig, x: i64, cancel: bool) -> usize {
    let Rig { context, gpu, .. } = rig;
    let staged = gpu.views[0].compaction.observed.staged.get();
    let frame = unlit(pose_at(x));
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let mut prepare = PrepareContext::new(context.device(), encoder)?;
            gpu.prepare_view_compaction(&mut prepare, None, &frame);
            prepare.finish();
            Ok::<_, TransactionError>(())
        })
        .unwrap();
    if cancel {
        prepared.cancel();
    } else {
        prepared.submit().unwrap();
    }
    gpu.views[0].compaction.observed.staged.get() - staged
}

fn flushed_rig() -> Rig {
    let mut rig = Rig::over(&two_meshes(), false);
    for _ in 0..3 {
        rig.render(&unlit(pose_at(0)));
    }
    rig
}

#[test]
fn moving_views_over_flushed_batches_ask_no_selected_batch() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = flushed_rig();
    let asked = rig.gpu.source_checks();
    let scans = rig.gpu.instances.flushed_scans();
    for x in [300, 900, 1_500, 2_100] {
        rig.render(&unlit(pose_at(x)));
    }
    assert_eq!(rig.gpu.source_checks(), asked);
    assert_eq!(
        rig.gpu.instances.flushed_scans(),
        scans,
        "the batches were scanned"
    );
    assert!(rig.gpu.views[0].compaction.observed.staged.get() > 0);
}

#[test]
fn an_unselected_unflushed_batch_does_not_stop_a_view_from_gathering() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = flushed_rig();
    rig.gpu.add_instance(of_second_mesh(0), false).unwrap();
    let batch = rig.gpu.instances.batches.iter().find(|(key, _)| key.1 == 1);
    assert!(batch.is_some_and(|(_, batch)| batch.buffer.is_none()));
    let staged = gather_unflushed(&mut rig, 900, false);
    assert!(staged > 0, "the flushed batches were not gathered");
    assert!(rig.gpu.committed_compaction_key(0).is_some());
}

#[test]
fn a_selected_unflushed_batch_stages_nothing_and_keeps_the_committed_gather() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = flushed_rig();
    let committed = rig.gpu.committed_compaction_key(0);
    rig.gpu.add_instance(of_second_mesh(0), true).unwrap();
    let staged = gather_unflushed(&mut rig, 900, false);
    assert_eq!(staged, 0);
    assert_eq!(rig.gpu.committed_compaction_key(0), committed);
    // The next frame flushes the new batch, and then the view gathers it too.
    rig.render(&unlit(pose_at(900)));
    assert_ne!(rig.gpu.committed_compaction_key(0), committed);
    assert!(rig.gpu.views[0].compaction.observed.staged.get() > 0);
    // Once flushed, the batches are no longer asked for one by one.
    let asked = rig.gpu.source_checks();
    rig.render(&unlit(pose_at(1_500)));
    assert_eq!(rig.gpu.source_checks(), asked);
}

#[test]
fn a_batch_started_after_a_flushed_scene_is_not_taken_for_flushed() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = flushed_rig();
    // One view of the flushed scene first, so any answer remembered for it is stale below.
    assert!(gather_unflushed(&mut rig, 300, false) > 0);
    rig.gpu.add_instance(of_second_mesh(1), true).unwrap();
    assert_eq!(gather_unflushed(&mut rig, 300, false), 0);
}

#[test]
fn a_flushed_scene_is_scanned_once_per_state_and_again_after_each_change() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = flushed_rig();
    let mut scans = rig.gpu.instances.flushed_scans();
    let mut rescanned = |rig: &mut Rig, x: i64| {
        // Two views of one state scan at most once between them.
        rig.render(&unlit(pose_at(x)));
        rig.render(&unlit(pose_at(x + 400)));
        let now = rig.gpu.instances.flushed_scans();
        let delta = now - scans;
        scans = now;
        delta
    };
    assert_eq!(rescanned(&mut rig, 300), 0);
    let handle = rig.gpu.instance_handles().nth(7).unwrap();
    let mut transform = *object(7).transform();
    transform.position_millimeters[0] += 600;
    rig.gpu.set_instance_transform(handle, transform).unwrap();
    assert_eq!(rescanned(&mut rig, 900), 1, "an edit");
    for index in 0..40 {
        rig.gpu.add_instance(object(index), true).unwrap();
    }
    assert_eq!(rescanned(&mut rig, 1_500), 1, "growth");
    assert_eq!(rescanned(&mut rig, 2_100), 0);
}

#[test]
fn previous_buffers_are_part_of_what_flushed_means() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = flushed_rig();
    let instances = &rig.gpu.instances;
    assert!(instances.all_flushed(false));
    assert!(
        !instances.all_flushed(true),
        "no batch keeps previous buffers"
    );
    assert!(
        instances.all_flushed(false),
        "the answer for another request"
    );
}
