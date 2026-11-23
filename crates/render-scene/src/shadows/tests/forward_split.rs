//! Where the forward frame's order comes from when shadows fit to scene depth. The
//! depth-fitted frame runs a prefix the scheduler authors (`encode_depth_prefix`), then a
//! suffix that the forward order sorts; an unfitted frame is one sorted list. Both ran in
//! these orders when the suffix was sorted by the `PassSlot` enum, and must still.
use super::*;
use crate::compositor::FrameRenderer;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_gpu::submission::PrepareError;

/// The passes `encode_depth_prefix` authors for the screen-effects frame: the prepass,
/// then the first screen stage (the linear depth level).
const PREFIX: [PassSlot; 2] = [PassSlot::DepthNormalPrepass, PassSlot::DepthPyramid(0)];

fn attachments(frame: &FrameRenderer) -> (AttachmentView, AttachmentView) {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    (color, depth)
}

fn executed_during(frame: impl FnOnce()) -> Vec<PassSlot> {
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    frame();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(std::mem::take)
}

fn fitted(adoption: AdoptionPolicy, resident: &mut ShadowedScene, context: &GpuContext) {
    resident
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: adoption,
                range: super::sdsm::policy(),
            }),
        )
        .unwrap();
}

/// The executed roles of one depth-fitted frame.
fn fitted_frame(context: &GpuContext, adoption: AdoptionPolicy) -> Vec<PassSlot> {
    let (mut frame, mut resident) = super::sdsm::screen_renderers(context);
    fitted(adoption, &mut resident, context);
    let (color, depth) = attachments(&frame);
    executed_during(|| {
        resident
            .render_depth_fitted_host(
                context,
                &view(),
                (&color, &depth),
                (Some(3), false, &[]),
                &mut frame,
            )
            .unwrap();
    })
}

const ADOPTIONS: [AdoptionPolicy; 2] = [
    AdoptionPolicy::SameFrameCapture,
    AdoptionPolicy::Interactive {
        latency_frames: 2,
        contraction_frames: 0,
    },
];

#[test]
fn gpu_fitted_frame_runs_the_authored_prefix_then_the_suffix_in_slot_order() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for adoption in ADOPTIONS {
        let executed = fitted_frame(&context, adoption);
        assert_eq!(
            executed[..PREFIX.len()],
            PREFIX,
            "{adoption:?}: the prefix runs as authored, ahead of the shadow casters that the \
             enum sorts first"
        );
        // The suffix is the frame minus what the prefix completed, sorted by slot.
        let suffix = &executed[PREFIX.len()..];
        let mut by_slot = suffix.to_vec();
        by_slot.sort();
        assert_eq!(suffix, by_slot, "{adoption:?}: the suffix is in slot order");
        assert_eq!(suffix.first(), Some(&PassSlot::ShadowCasters(0)));
        assert!(suffix.contains(&PassSlot::Opaque), "{adoption:?}");
        assert!(
            !suffix.iter().any(|slot| PREFIX.contains(slot)),
            "{adoption:?}: nothing the prefix completed runs again"
        );
        let mut today = PREFIX.to_vec();
        today.extend(by_slot);
        assert_eq!(executed, today, "{adoption:?}");
    }
}

#[test]
fn gpu_unfitted_frame_runs_one_list_in_slot_order() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut frame, mut resident) = super::sdsm::screen_renderers(&context);
    assert_eq!(resident.depth_fitting_settings(), None);
    let (color, depth) = attachments(&frame);
    let executed = executed_during(|| {
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                resident.encode_with(&context, &view(), (&color, &depth), encoder, &mut frame)
            })
            .map_err(PrepareError::into_error)
            .unwrap();
        prepared.submit().unwrap();
    });
    let mut today = executed.clone();
    today.sort();
    assert_eq!(executed, today, "with no prefix the whole frame is sorted");
    assert_eq!(executed.first(), Some(&PassSlot::ShadowCasters(0)));
    assert_eq!(
        executed
            .iter()
            .filter(|&&slot| slot == PassSlot::DepthNormalPrepass)
            .count(),
        1
    );
    assert!(executed.contains(&PassSlot::Opaque));
}
