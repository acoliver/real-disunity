//! Where the forward frame's order comes from when shadows fit to scene depth. The
//! depth-fitted frame runs a prefix the scheduler authors (`encode_depth_prefix`), then a
//! suffix that the forward order sorts; an unfitted frame is one sorted list. Both ran in
//! these orders when the suffix was sorted by the `PassSlot` enum, and must still.
use super::*;
use crate::compositor::FrameRenderer;
use crate::gbuffer::{GBUFFER_FORMATS, Gbuffer, GbufferUse};
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_gpu::submission::PrepareError;
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PrepareContext,
    PreparedFeature,
};

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

/// A producer in the G-buffer slot that writes nothing and records that its pass ran, so a
/// frame's executed order shows where the G-buffer sits. The slot's own pass has no
/// producer until one registers.
struct GbufferRecorder;

struct GbufferRecorderDraw;

impl FrameFeature for GbufferRecorder {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::OpaqueGbuffer);
        declaration.passes[0].color_format = Some(GBUFFER_FORMATS[0]);
        declaration.passes[0].additional_color_formats = GBUFFER_FORMATS[1..].to_vec();
        declaration
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(GbufferRecorderDraw))
    }
}

impl PreparedFeature for GbufferRecorderDraw {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        crate::scene_metadata::record_pass(context.slot);
    }
}

/// The screen-effects renderers with the G-buffer observation's targets and a producer in
/// its slot, as `SceneOpaquePath::GBUFFER_OBSERVATION` configures the frame.
fn observing_renderers(context: &GpuContext) -> (FrameRenderer, ShadowedScene) {
    let (mut frame, resident) = super::sdsm::screen_renderers(context);
    let viewport = view().viewport();
    let format = frame.targets().config().format;
    frame.gbuffer = Some(Gbuffer::new(
        context.device(),
        [viewport.width, viewport.height],
        (format, false),
        GbufferUse::Observe,
    ));
    frame
        .register(context.device(), Box::new(GbufferRecorder))
        .unwrap();
    (frame, resident)
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

/// The executed roles of one depth-fitted frame, with the G-buffer observation on or off.
fn fitted_frame(context: &GpuContext, adoption: AdoptionPolicy, observed: bool) -> Vec<PassSlot> {
    let (mut frame, mut resident) = if observed {
        observing_renderers(context)
    } else {
        super::sdsm::screen_renderers(context)
    };
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
        let executed = fitted_frame(&context, adoption, false);
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

#[test]
fn gpu_observed_gbuffer_runs_before_opaque_and_leaves_the_depth_prefix_split_unchanged() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for adoption in ADOPTIONS {
        let without = fitted_frame(&context, adoption, false);
        let observed = fitted_frame(&context, adoption, true);
        let gbuffer = observed
            .iter()
            .position(|&slot| slot == PassSlot::OpaqueGbuffer)
            .unwrap_or_else(|| panic!("{adoption:?}: the G-buffer slot did not run"));
        assert_eq!(
            observed
                .iter()
                .filter(|&&slot| slot == PassSlot::OpaqueGbuffer)
                .count(),
            1,
            "{adoption:?}"
        );
        assert_eq!(
            observed.get(gbuffer + 1),
            Some(&PassSlot::Opaque),
            "{adoption:?}: the G-buffer runs immediately before Opaque"
        );
        let mut others = observed.clone();
        others.remove(gbuffer);
        assert_eq!(
            others, without,
            "{adoption:?}: the observation adds the G-buffer and moves nothing else"
        );
        assert_eq!(observed[..PREFIX.len()], PREFIX, "{adoption:?}");
        let suffix = &observed[PREFIX.len()..];
        let mut by_slot = suffix.to_vec();
        by_slot.sort();
        assert_eq!(suffix, by_slot, "{adoption:?}: the suffix is in slot order");
    }
}

#[test]
fn gpu_observed_unfitted_frame_runs_the_gbuffer_immediately_before_opaque() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let run = |observed: bool| {
        let (mut frame, mut resident) = if observed {
            observing_renderers(&context)
        } else {
            super::sdsm::screen_renderers(&context)
        };
        let (color, depth) = attachments(&frame);
        executed_during(|| {
            let prepared = context
                .submission()
                .prepare(context.device(), |encoder, _| {
                    resident.encode_with(&context, &view(), (&color, &depth), encoder, &mut frame)
                })
                .map_err(PrepareError::into_error)
                .unwrap();
            prepared.submit().unwrap();
        })
    };
    let without = run(false);
    let observed = run(true);
    let gbuffer = observed
        .iter()
        .position(|&slot| slot == PassSlot::OpaqueGbuffer)
        .expect("the G-buffer slot ran");
    assert_eq!(observed.get(gbuffer + 1), Some(&PassSlot::Opaque));
    let mut others = observed.clone();
    others.remove(gbuffer);
    assert_eq!(others, without, "the observation moves nothing else");
    let mut by_slot = observed.clone();
    by_slot.sort();
    assert_eq!(
        observed, by_slot,
        "with no prefix the whole frame is sorted"
    );
}
