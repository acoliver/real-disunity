//! What the persistent targets of a game report while a frame is in preparation. A fitted capture
//! submits the frame in two segments, and the first one records what it wrote before the second
//! one plans, so the second must still see the targets as the frame found them.
use super::targets::FrameEffect;
use super::{ColorOutput, GamePassSpec, GameTargets, TargetDeclaration, TargetExtent, TargetStore};
use crate::device::GpuContext;
use std::sync::Arc;
use worldrunner_render_gpu::submission::{
    Publications,
    capture::{CaptureError, CaptureWrites, WriteKey},
};
use worldrunner_render_post::{FrameError, GamePlace, PassDeclaration, PassSlot};

const KEPT: &str = "frame start kept";

struct Rig {
    context: GpuContext,
    store: Arc<TargetStore>,
    targets: GameTargets,
}

impl Rig {
    fn new() -> Self {
        let context = GpuContext::shared_for_test().unwrap();
        let declaration = TargetDeclaration::new(
            KEPT,
            wgpu::TextureFormat::Rgba8Unorm,
            TargetExtent::Fixed([4, 4]),
        )
        .layers(2)
        .persistent();
        let store = Arc::new(TargetStore::build(
            context.device(),
            None,
            &[declaration],
            [8, 8],
        ));
        let targets = GameTargets::new(store.clone());
        Self {
            context,
            store,
            targets,
        }
    }

    fn key(&self) -> WriteKey {
        self.store.backing(KEPT).unwrap().write_key()
    }

    /// What the commit of a game feature that wrote layer 0 stages in `frame`.
    fn stage_layer_zero(&self, frame: &mut Publications) {
        FrameEffect {
            backing: self.store.backing(KEPT).unwrap().clone(),
            written: vec![((0, 0), true)],
        }
        .stage(frame);
    }

    /// Runs one frame as a live transaction, which publishes when it is submitted.
    fn transaction(&self, plan: impl FnOnce(&GameTargets)) {
        let authority = self.context.submission();
        authority
            .prepare(self.context.device(), |_, _| {
                plan(&self.targets);
                Ok::<_, FrameError>(())
            })
            .unwrap()
            .submit()
            .unwrap();
        self.settle();
    }

    /// Waits for the submitted work, which a following capture needs.
    fn settle(&self) {
        self.context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }

    /// Runs one frame as an owned capture: `prefix` plans and stages in the first segment, which
    /// is submitted before `suffix` plans and stages in the second.
    fn capture(
        &self,
        prefix: impl FnOnce(&GameTargets, &mut Publications),
        suffix: impl FnOnce(&GameTargets, &mut Publications) -> Result<(), FrameError>,
    ) -> Result<(), FrameError> {
        let keys = [self.key()];
        let writes = CaptureWrites::new(keys, keys, keys).unwrap();
        let device = self.context.device();
        let first = self
            .context
            .submission()
            .begin_capture(device, writes, (), |_, frame, ()| {
                prefix(&self.targets, frame);
                Ok::<_, FrameError>(())
            })
            .unwrap();
        let second = first.submit().unwrap().resume(
            device,
            |_, _| Ok::<_, FrameError>(()),
            |_, frame, (), ()| suffix(&self.targets, frame),
        );
        match second {
            Ok(second) => {
                second
                    .submit()
                    .unwrap()
                    .finish(device, |_, _| Ok::<_, FrameError>(()))
                    .unwrap();
                self.settle();
                Ok(())
            }
            Err(CaptureError::Preparation(error)) => Err(error),
            Err(error) => panic!("the capture failed outside the suffix: {error:?}"),
        }
    }
}

/// The pass of a game that writes layer 0, as a feature plans it into the frame's list.
fn writer(targets: &GameTargets) -> PassDeclaration {
    let target = TargetDeclaration::new(
        KEPT,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    );
    let mut passes = vec![PassDeclaration::new(PassSlot::Opaque)];
    let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
        .writing([ColorOutput::Target(target.layer(0))]);
    targets
        .plan_pass(&spec, &mut passes, &mut Vec::new())
        .unwrap();
    passes.remove(1)
}

fn resource(layer: u32) -> worldrunner_render_post::ResourceRef {
    worldrunner_render_post::ResourceRef {
        name: KEPT,
        version: 0,
        mip: 0,
        layer,
    }
}

#[test]
fn the_segments_of_a_capture_plan_the_pass_the_first_one_completed_identically() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    let mut completed = None;
    let mut replanned = None;
    rig.capture(
        |targets, frame| {
            completed = Some(writer(targets));
            rig.stage_layer_zero(frame);
        },
        |targets, frame| {
            replanned = Some(writer(targets));
            rig.stage_layer_zero(frame);
            Ok(())
        },
    )
    .unwrap();
    // The executor completes a prefix entry for the suffix only when the suffix lists it with
    // the same attachments, load included.
    let (completed, replanned) = (completed.unwrap(), replanned.unwrap());
    assert_eq!(replanned.color, completed.color);
}

#[test]
fn the_second_segment_of_a_capture_plans_against_the_state_the_frame_started_with() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    rig.capture(
        |_, frame| rig.stage_layer_zero(frame),
        |targets, _| {
            assert!(
                !targets.is_written(KEPT, 0),
                "the first segment's write is not visible to the plan"
            );
            assert!(!targets.view(resource(0)).unwrap().retained());
            assert!(!targets.view(resource(1)).unwrap().allocation_initialized());
            assert!(
                rig.store
                    .imports()
                    .iter()
                    .all(|(_, initial)| initial.is_none()),
                "the frame imports the target the way its capture did"
            );
            Ok(())
        },
    )
    .unwrap();
    assert!(rig.targets.is_written(KEPT, 0), "the next frame sees it");
}

#[test]
fn a_later_frame_sees_what_a_capture_published() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    rig.capture(|_, frame| rig.stage_layer_zero(frame), |_, _| Ok(()))
        .unwrap();
    rig.transaction(|targets| {
        assert!(targets.is_written(KEPT, 0));
        assert!(!targets.is_written(KEPT, 1));
        assert!(targets.view(resource(0)).unwrap().retained());
        assert!(targets.view(resource(1)).unwrap().allocation_initialized());
    });
    rig.capture(
        |targets, _| assert!(targets.is_written(KEPT, 0), "the next capture's prefix"),
        |targets, _| {
            assert!(targets.is_written(KEPT, 0), "the next capture's suffix");
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn a_capture_whose_suffix_fails_leaves_the_published_state_to_the_next_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    let refused = rig.capture(
        |_, frame| rig.stage_layer_zero(frame),
        |targets, _| {
            assert!(!targets.is_written(KEPT, 0));
            Err(FrameError::MissingPass(
                worldrunner_render_post::PassEntry::new(PassSlot::Opaque),
            ))
        },
    );
    assert!(refused.is_err());
    // The first segment was submitted, so its write is real; no snapshot of the failed frame
    // answers for the frame after it.
    assert!(rig.targets.is_written(KEPT, 0));
    rig.transaction(|targets| assert!(targets.is_written(KEPT, 0)));
}

#[test]
fn a_transaction_sees_the_state_it_started_with_and_publishes_on_submission() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    rig.transaction(|targets| {
        assert!(!targets.is_written(KEPT, 0));
    });
    assert!(!rig.targets.is_written(KEPT, 0), "nothing was staged");
    let authority = rig.context.submission();
    let prepared = authority
        .prepare(rig.context.device(), |_, frame| {
            rig.stage_layer_zero(frame);
            assert!(!rig.targets.is_written(KEPT, 0), "staged, not published");
            Ok::<_, FrameError>(())
        })
        .unwrap();
    assert!(!rig.targets.is_written(KEPT, 0), "prepared, not submitted");
    prepared.submit().unwrap();
    assert!(rig.targets.is_written(KEPT, 0), "submitted");
}

#[test]
fn snapshots_of_finished_frames_are_not_kept() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let rig = Rig::new();
    for _ in 0..3 {
        rig.capture(|_, frame| rig.stage_layer_zero(frame), |_, _| Ok(()))
            .unwrap();
    }
    rig.transaction(|_| ());
    let authority = rig.context.submission();
    authority
        .prepare(rig.context.device(), |_, frame| {
            rig.stage_layer_zero(frame);
            assert_eq!(
                super::frame_start::kept(),
                1,
                "only the frame in preparation keeps a snapshot"
            );
            Ok::<_, FrameError>(())
        })
        .unwrap()
        .cancel();
}
